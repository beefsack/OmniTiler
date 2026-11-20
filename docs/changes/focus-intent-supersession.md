# Retained focus intent supersession

Unit A, approved 2026-11-20: one retained focus request and its apply fence. A host side effect of applying our decision is not a new user choice; this is not a general input-provenance framework.

## Approved rule and consequences

> **A pending focus request stays until its target gets focus or a newer intent replaces it. Host changes that only happened because we applied our own decision are not intent.**
> 1. “Newer” = observed after the request’s issuing report, in platform capture order. No timestamps or timing.
> 2. Intent is judged against core-retained state at execution (existing principle, docs/principles.md “Ordering and Races”). So a click on a window that an already-accepted switch hides is ineligible, and the switch’s focus target wins. A click on a destination window after the switch is newer and wins. A native user desktop switch is newer intent and cancels a request whose target it hides.
> 3. The platform tags foreground changes caused by its own native call causally, never by timing. Unknown cause = intent (never steal focus; today’s behaviour). Hosts that can’t tag document it as a host limitation.

Keep satisfaction, target loss, and terminal safety/refusal settlement independent of intent.

| Case | Today (verified source unless noted) | Proposed |
| --- | --- | --- |
| Our selection/follow switches views and the host activates fallback C, not T | Live selection retired `baseline-changed` before a setter; live follow only proved the equality branch (`docs/changes/kde-rebuild-v4.md:419–439`). The same retirement can affect follow by inspection (`crates/tiler-core/src/world_close_focus.rs:490–503`; `kwin/src/world-apply.ts:583–622,1882–1917`). | Keep T's id and dependencies: C is foreground, not a new choice. After readiness, at most one safe setter; zero if already T. |
| Our send, minimise or output move of the focused client activates another | Same cause-blind retirement. KDE wraps membership/output writes for homing, not foreground causality (`crates/tiler-core/src/world_close_focus.rs:490–503`; `kwin/src/world-entry.ts:2721–2734,6218–6227`). At KWin `1f543de908`, minimise can activate a successor (`src/xdgshellwindow.cpp:673–681`, `src/x11window.cpp:1201–1209`); fallback is not established for every send/output move or a controlled KDE minimise route (`src/window.cpp:694–737,3963–3996`; `kwin/src/world-observe.ts:1275`). | A proved own fallback does not cancel still-eligible T. Genuine target ineligibility still settles; unproved fallback is not exempt. |
| Eligible D activates while T is in flight | Changed sampled foreground retires T, but intermediate activation can disappear between snapshots. KWin invalidates on unrecognised activation; only close has a direct foreground comparison at the setter fork (`crates/tiler-core/src/world_close_focus.rs:490–503`; `kwin/src/world-entry.ts:7935–7955`; `kwin/src/world-apply.ts:2880–2912`). | D retires T; no compensating setter, even if a later activation hides D's evidence in the final snapshot. |
| Click S while an accepted switch will hide it | Changed S can retire T before view reconciliation. The ordinary effective-view lookup may still prefer the native source echo (`crates/tiler-core/src/world_run_v4.rs:218–229`; `world_close_focus.rs:490–503`; `world_observe.rs:159–180`). KWin's stopped wrapper may spend after invalidation (`kwin/src/world-apply.ts:2946–2989`). | Against the retained destination, S is ineligible; keep T. If the old apply is invalid, defer unspent. A click on eligible destination D wins. |
| Native switch hides T without changing a sticky foreground | The map-wide drift guards can also retire on unrelated changes (`crates/tiler-core/src/world_close_focus.rs:504–525,766–778`); view reconciliation can adopt a changed echo (`world_settle.rs:613–645,665–713`). | Positive native selector change hiding T retires it independently of foreground. Unrelated-output drift and same-desired-view confirmation do not. |

## Why one report interval

- **Attempt-row foreground `pre/post`:** reuses brackets such as `ViewAttempt.pre/post/base` (`kwin/src/world-apply.ts:599–621,1881–1904`; `kwin/src/world-state.ts:628`), but post C and report C do not distinguish C→D→C. A prior attempt is not proof of continuity.
- **Current-foreground flag:** a cheap `windowActivated` dirty mark outside the validated own-call scope distinguishes C→D→C from uninterrupted C, but cannot distinguish `[S]` from `[D,S]` or carry a selector change with unchanged focus (`kwin/src/world-entry.ts:2735–2754,7935–7974`). Keep this event hook, not a wire flag.
- **Selected: required nullable interval.** Snapshot-only, complete, and gap are distinct strict-codec cases. For C→D→C, record D and C unless the return to C is proved own; in either case D remains. Final foreground stays in `focused`/`foregroundKnown`.

**Exact binding (source-verified).** `obs` is a watermark (`docs/porting/world-contract.md:394`), and core refuses only a lower one (`crates/tiler-core/src/world_admit_v4.rs:536–545`). `reqSeq` *is* strictly increasing across fresh incorporations: lower is stale, and equal is a replay or a reuse error; only a committed report replaces the replay cache (`world_admit_v4.rs:252–277,498–529,1007–1022`). The broker allocates `reqSeq` before decode and doesn't echo it to KWin (`crates/omnitiler/src/world_broker.rs:929–960,1306–1321,1625–1628`; `crates/tiler-core/src/world_report.rs:2681–2698`). Use the existing **incorporated reply `rev`** for `after`: a fresh full incorporation increments it exactly once, bootstrap doesn't, and exact replay returns the same publication (`world_admit_v4.rs:518–527,756–816,992–1022`; `crates/tiler-core/src/world_run_v4.rs:873–874`). This identifies the exact predecessor on both routes without a broker translation or new ACK field.

**KWin attribution, verified in source at `1f543de908`, not live.** `setCurrentDesktop` calls `setCurrent` and emits `currentChanged` in-stack (`src/scripting/workspace_wrapper.cpp:74–81`; `src/virtualdesktops.cpp:577–601`; `src/workspace.cpp:210–216`). Its slot restores focus on the active output (`src/workspace.cpp:1051–1054,1103–1156`); `requestFocus` synchronously updates compositor active window and emits `windowActivated` (`src/activation.cpp:223–254,272–279,366–407`; relay `src/scripting/workspace_wrapper.cpp:34–42`). Inside the call is necessary, not sufficient: reuse `ViewFocusScope` with expected target/view, exact lifetime and post-call readback, not a global in-setter bit (`kwin/src/world-entry.ts:2735–2754,2822–2907,2922–3027`). Other mutations require their own source proof before exclusion. A click on an already-active client with no observable event is a host limitation (`src/activation.cpp:223–227,278`, same pin).

**X11/Xwayland:** `X11Window::takeFocus` queues X input focus and/or `WM_TAKE_FOCUS`; later `FocusIn` can update compositor activation (`1f543de908`, `src/x11window.cpp:1631–1647`, `src/events.cpp:343–345,532–595`; Xwayland startup `src/main_wayland.cpp:166–185`). Synchronous compositor foreground is distinct from client keyboard delivery. Late events without causal proof stay in the list; actual client interleavings are **unverified**.

## Contract and execution cursor

The [v4 contract](../porting/world-contract.md#worldreport) defines the required
nullable interval and the [shared supersession and apply rules](../porting/world-contract.md#one-retained-focus-request).
No capability, attempt-row changes, current-foreground label, or new ACK protocol.

Only an unsettled focus request, including an earlier accepted choice in the
current FIFO batch, can make the execution cursor differ from eligible native
foreground. Otherwise use eligible observed foreground. D followed by proved-own
T retires the T request and uses native T as the next `toggle-float` subject;
neither D nor T gets a compensating setter. Rapid focus-directional followed by
`toggle-float` without intervening activation uses the new logical target.
The narrow newer-report exception for an unavailable pending target and
same-report positive-close correction and unknown-foreground fence are unchanged.

## Implementation inventory

- **Core:** add the report fact (`crates/tiler-core/src/world_report.rs:1423`; `world_observe.rs:218`) and check the predecessor at admission. Split origin/equality/terminal settlement from supersession in `world_close_focus.rs:395–525`. Judge eligibility from the retained selection before resolving commands, not from the echo-first `effective_view`; preserve live pending cursor without baseline equality (`world_close_focus.rs:199–230`; `world_run_v4.rs:218–229,398–445`). Adjust the canonical/cursor branch so snapshot truth cannot undo adjudication.
- **View drift:** remove both map-wide focus guards (`world_close_focus.rs:504–525,766–778`): they can over-cancel on unrelated changes. The original healthy switch itself does not trip them: select/follow writes the retained goal before mint copies `view_basis`, and successful native settlement re-inserts the same goal (`world_tiling_views.rs:197–201,277–292`; `world_sends.rs:375–419`; `world_run_v4.rs:751–765`; `world_close_focus.rs:570–582`; `world_settle.rs:548–573`). Test relevant visibility, not arbitrary map inequality.
- **Protocol/KWin:** strict report decoding, canonical encoding and sizing (`crates/tiler-protocol/src/world_protocol.rs:3954–3963,4381,6840`); no handshake or attempt change. Extend the scoped guard and observation encoder (`kwin/src/world-entry.ts:2735–2754,7917–7974`; `kwin/src/world-observe.ts:836–873`; `kwin/src/world-report.ts:3069,3249–3253`). Reuse the frozen-prefix cut at `kwin/src/world-entry.ts:6502–6507,6923–6945`: matching incorporation retires only delivered evidence, before any apply skip. Move the ordinary spend past the final reentrant checks, and prevent stopped-wrapper spend on adjudication deferrals (`kwin/src/world-apply.ts:2777–2779,2855–2912,2946–2989`).
- **Unit B (separate change):** [Seal native shortcut cuts](kde-shortcut-native-cuts.md), splitting at intervening foreground/view transitions or gaps. Proved-own transitions split cuts without adding wire events. Discriminator: D active → `toggle-float` → proved-own T → `toggle-float` acts on D then T.
- **Fixtures:** strict in-place prerelease v4 amendment, not wire-compatible, no dual decoder. Keep `world-wire/v4`, `stage5`, `stage5-v4` and `kde-v4` byte-identical to `ff6eaddc` (`docs/porting/world-contract.md:8,42–56,1969–1979`; `docs/changes/kde-rebuild-v4.md:40–47`). Re-record only affected active vectors in `world-wire/v4-active`, `stage5-v4-active` and `kde-v4-active` using real producers (`crates/tiler-protocol/tests/world_codec.rs:1020–1055`; `crates/omnitiler/tests/stage5-fixture-route.rs:4–16`; `kwin/tests/world-stage5-fixture.test.ts:10–43`; `kwin/tests/world-bytes.test.ts:3–22`).

**Other hosts (source-only):** Windows' `landed-own` is an identity join that cannot distinguish a same-target click (`crates/tiler-windows/src/tiling_sys/world_diff.rs:1189–1199,2567–2639`).
macOS AX notifications coalesce into rereads; a tagged injected Space shortcut does not tag a later foreground observation (`crates/tiler-macos/src/native_tile_backend.rs:1840–1876`; `input_match.rs:89–119`; `ffi.rs:10135–10143`).
macOS produces snapshot-only v4 reports; Windows has no `WorldReportV4` producer to migrate. Native verification remains deferred ([host status](../porting/world-contract.md)).

The i3 4.25.1 and sway 1.12 comparisons are in the
[reference matrix](../spec/reference-outcomes.md#retained-focus-supersession).
They are source evidence only; overlap outcomes remain TBD.

## Acceptance

1. First red: `selection_focus_survives_own_view_switch_fallback` in `crates/omnitiler/tests/focus_arrival.rs` (checked `WorldService` helpers at `:97–195`; related send-follow outcome at `:382–467`). Seed two bound desktops, remember T on the destination alongside C, return to source, and issue select. Skip the issuing apply; report destination shown with raw view readback and C foreground before any focus feedback. First reproduce today's `baseline-changed` with today's valid wire; then supply a complete empty interval at cutover and require unchanged id/rev, eventual `complete/proven`, and no premature completion. Keep the snapshot-only negative control.
2. `[S]` retains T, `[D,S]` retires T with the same final S; no resurrection. Also cover C→D→C, destination click, own send/follow/minimise/output fallback versus unknown, target loss, and same-report close correction.
3. Unrelated-output switch vs target-hiding native switch with unchanged sticky foreground; include same-desired-view confirmation.
4. Lose the ACK, capture an event during flight, replay exact bytes, and confirm that only the acknowledged prefix retires even on skipped apply. Test interrupted/overflow gap, equal `obs` on distinct incorporations, predecessor mismatch and strict codec shapes.
5. D then own T: T retired, zero compensating setters; assert the selected implicit-command cursor behavior.
6. KWin ordinary and stopped-wrapper paths: unproved input defers unspent, including input during the last reentrant read; S permits the same id on a later eligible reply, D makes no T call. Proved own fallback permits one setter after readiness; late unproved X activation is not suppressed.
7. Rapid focus-directional then `toggle-float` with no intervening activation acts on the new logical target; the Unit B press-time discriminator is deferred.
8. Scoped Rust fmt, strict all-target clippy and serial all-target tests; serial KWin suite with Qt probes; frozen bytes; Callgrind and report-scale p95 caps. Run one fixture-scoped live KDE selection with destination fallback C distinct from T: same request id/rev, exactly one setter, readback proves T and first terminal `SwitchEvent complete/proven`.

### Old-wire reproduction

The checked `WorldService` regression failed before the cutover as expected:
the selection issued `focus-5` at revision 5 for `win-t`; revision 6
reported the view readback and C as foreground, with no focus feedback.
Core logged `id=focus-5 target=win-t issuingRev=5 decision=no-call reason=baseline-changed`.
The regression expected `(focus-5, 5)` and received `(None, None)`.

On the amended wire, the checked route retains `(focus-5, 5)` through the
fallback with a complete empty interval. The switch is not complete on view
readback alone; it becomes `complete/proven` on the correlated arrival.
The snapshot-only negative control still retires the request.

**Reachability:** the proposed deferral rules out a new focus setter after captured, unadjudicated D on both focus paths. It does not by itself fence all view/membership/state writes: source shows separate view writes before focus and a separate native-state executor (`kwin/src/world-apply.ts:1881–1917,2734–2779`; `kwin/src/world-entry.ts:6206–6237`), and view fallback runs on the active output only (KWin `1f543de908`, `src/workspace.cpp:1103–1130`). Whether one of those paths or a previously issued native operation can produce a **proved** own T after D in the same interval under the new fences is **unverified**. Keep the synthetic test; do not widen apply authority to manufacture the case.

Subjective transition jank is deferred to in-person testing: look for flicker or a noticeable dwell on fallback C before T takes focus.

### Unit A verification

On the post-QV4-fix tree, scoped Rust formatting and all-target Clippy
with warnings denied passed. The serial Rust all-target gate passed
3,566 tests across 63 binaries using a fresh temporary directory, with
none failing, ignored, or filtered out. The four frozen fixture families
are byte-identical to `ff6eaddc`.

KWin type checking and the serial npm suite with the offscreen Qt probes
passed: 1,417 tests across 203 suites, no failures or skips. The standalone
shipped-bundle QV4 CTest also passed (1/1).

`bash scripts/check-report-cost.sh` passed: 7.83M and 34.64M
instructions/report at 50 and 200 windows, with 4.42× growth (cap 6.5×).
`report_scale all 31 3 all` worst p95 across measured shapes was
0.2241/1.7755/4.5841 ms at 10/50/200 windows (caps 1/4/10 ms).

### Real KDE selection: pre-fix failures

One fixture-only selection leg repeated the Unit 8 C≠T setup. The issuing
and intermediate applies were skipped. Decisive trace fields (native window
IDs substituted with A, C and T):

```text
describe-op lane=focus rev=10 op=focus-10 target=T afterShownOp=op-2
describe-op lane=focus rev=11 op=focus-10 target=T afterShownOp=op-2
describe-op lane=focus rev=12 op=focus-10 target=T afterShownOp=op-2
observe-applied rev=12 focusFresh=A focusPost=C
fixture-view-attempt-sent seq=13 op=op-2 setter=accepted
focus-eval rev=13 id=focus-10 target=T issuingRev=10 decision=no-call reason=foreground-intent
describe-receipt rev=13 ready=1 focusReq=0 settledSwitches=0
```

The request survived skipped apply, but the real fallback was not omitted as
proved own. No T setter, arrival proof or terminal `SwitchEvent` was seen.
The traces do not distinguish which attribution guard left the event unproved.
Raw logs are preserved in the ignored run results.
Teardown removed the fixture and owned desktops, restored the original roster
with workspace 1 current and reactivated the exact starting terminal.

The pin-order offline test (C activates before scripting exports the selector
change, with braced native UUIDs) passed. A single diagnostic live leg then
confirmed that C entered the in-stack `own scope=view` branch. The next
capture sent exactly `foreground C` and `view destination` (`after=13`,
`gap=0`, `count=2`). Core's `focus-foreground-intent` selected `eventIndex=0`
and retired `focus-11` at revision 14. No scope-exit verdict was logged
for that switch or for an earlier direct focus whose own activation also
escaped. The evidence did not support a second activation or a core decode
error. The original desktop roster and terminal were restored.

### Qt 6.12.0 scope cleanup

An offscreen `QJSEngine` using the same Qt build reproduced the diagnostic
failure in the shipped JavaScript. QV4 restored the scope, then resumed the
pending return early after the nested `try`/`catch` inside `finally`, before
the scope could remove its provisional events. Minimal probes reproduced
this with pending return, `break`, or `continue` through a `finally` with a
nested `try`/`catch` or `for...of`. Pending throws ran the entire cleanup
and propagated the original exception. The three KWin suppression wrappers
now return only after `finally`; attribution and wire rules are unchanged.
No other shipped `finally` combined an exposed return, `break`, or
`continue` with either nested construct.

The standalone offscreen regression executes the wrappers from the actual
generated bundle, reentering the engine through a QObject signal. It checks
that only the provisional events are removed and that throwing setters
preserve exception identity. Before the fix it stopped at
`accepted view: exact provisional event splice` with both own events still
present. It passed on both fixed release and trace bundles, including with
the interpreter forced on the trace bundle. Run it after `npm run build`
from `kwin`, using Qt Core and Qml development libraries:

```sh
cmake -S tests/qv4 -B dist/qv4
cmake --build dist/qv4
ctest --test-dir dist/qv4 --output-on-failure
```

### Real KDE selection: passed after the QV4 fix

One rerun used the same fixture-only selection leg and C≠T setup. This time
the issuing and intermediate applies were skipped, the view switch proved
both its selector change and fallback C, and neither event went on the wire.
KWin called the T setter exactly once for `(focus-10, 10)`. Native
readback changed from C to T, and the first terminal switch receipt was
`complete/proven`. Decisive trace fields (native IDs substituted):

```text
describe-op lane=focus rev=10 op=focus-10 target=T afterShownOp=op-2
describe-op lane=focus rev=11 op=focus-10 target=T afterShownOp=op-2
describe-op lane=focus rev=12 op=focus-10 target=T afterShownOp=op-2
focus-scope-exit scope=view viewProved=1 activationProved=1
observe-applied rev=12 focusFresh=A focusPost=C
focus-changes-sent after=12 gap=0 count=0
focus-eval rev=13 id=focus-10 target=T issuingRev=10 decision=called reason=setter-accepted
fixture-focus-feedback-sent seq=14 id=focus-10 op=op-2 issuingRev=10 setter=accepted pre=C post=T proven=1
settlement-switch phase=receipt rev=14 op=op-2 outcome=complete focus=proven fallback=false failed=-
```

The fixture and planner exited. The original desktop roster was restored
with workspace 1 current, the exact starting terminal was reactivated,
and no fixture-scoped windows remained.
