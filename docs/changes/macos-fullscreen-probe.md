# macOS fullscreen return probe (operational record)

**Status:** operational host evidence only. No product route, wire, or core behavior was enabled. The design record is [the core proposal](macos-fullscreen-return-core-proposal.md).

## Scope and sample rules

Signed, SIP-on, host-only fixture runs exercised an owned windowed Desktop sibling and the same window's detached fullscreen Space through pinned Mission Control and the Dock gesture. A separate product switch-away candidate and a single signed rerun tested the product path; neither established a product route.

Timings are first **samples**, in µs from one common monotonic clock per run. A dash means not sampled, not that the host never emitted an event. The display/Space guard coalesces active-Space and display-parameter notifications, so it does not identify or time a Space notification alone. The log retains only first observations. Exact SLS current is physical readback; exact foreground is an independent AX observation and does not by itself prove physical arrival.

## Round-trip result

Both round trips completed with the same fixture lifetime and the same detached SID present while away and after both returns. All four legs held exact SLS arrival without bounce, preserved the other display's current, and issued no fullscreen-member writes during switching; the only fullscreen-member state requests were entry and teardown.

| Route and direction | Dispatch start | Display/Space guard | Exact SLS current | Exact foreground |
|---|---:|---:|---:|---:|
| Pinned Mission Control → Desktop | 1,323 | — | 810,908 | — |
| Pinned Mission Control → fullscreen | 3,933,392 | 4,657,459 | 4,696,186 | 4,702,120 |
| Dock gesture → Desktop | 5,135,013 | 5,347,558 | 5,371,603 | 5,389,578 |
| Dock gesture → fullscreen | 6,342,059 | 6,650,051 | 6,688,100 | 6,639,194 |

The windowed-sibling setup fell between the away and return observations (Mission Control: 3,293,295–3,932,081 µs; Dock: 5,798,988–6,340,867 µs). The Dock return foreground sample preceded exact SLS current, and the log has no post-current foreground bracket for that leg. This proves host switching on the tested layout, not unassisted Desktop focus for Mission Control or a general notification order.

## First signed attempt and signer mistake

The first signed attempt supplied only the body of the designated requirement instead of the complete `designated => …` line. It stopped at signature bind after disposable-Space setup and never entered fullscreen. The controller cleared that attempt's receipt with no new residue and no owned processes remained.

## First fullscreen run

The first fullscreen run observed the exact fixture window as the focused fullscreen AX window, attached exclusively to a new SLS fullscreen Space. That per-window read did not enumerate all members of the Space. Neither route was measured: the pre-dispatch sentinel proof could not name the window via `AXWindows`, and the same proof refused explicit exit. Teardown tried ordinary restoration before the detached SID retired. A read-only inspection found the starting native currents and order restored and Mission Control closed. This proves a distinct detached SID on entry, not its lifetime while away, round-trip behavior, or notification ordering.

After that run the user reported switching apps after a black teardown screen that showed only the Dock. The evidence-first archive recorded seven visibility differences under the explicit user drift waiver; it did not restore or touch those windows. A final read-only recheck found exact starting topology, no owned fixture lifetime or Space, and no blocking drift before the receipt was moved. Closing a still-fullscreen fixture window is a suspected cause of the black screen, not a measured host mechanism.

The fixture now requests exact windowed mode and waits for SLS to show the detached SID retired before closing the window. If retirement is not proved by the deadline, teardown requires a verified return to the saved starting current before allowing fixture shutdown.

## Corrected source guard

The corrected SIP-on run reached fullscreen. Pinned Mission Control switched away: exact SLS Desktop current was sampled at 952,876 µs on the probe clock (dispatch started at 1,124 µs); neither a display/Space guard notification nor exact Desktop-sibling foreground was sampled. The fixture app's AX focus still named the fullscreen member, not the Desktop sibling, so the return source guard refused before dispatch.

A bounded harness repair explicitly focused only the owned, windowed Desktop sibling **after** measuring away, then re-proved the exact source before dispatching return. No product guard changed. The signed retry then produced the four-leg result above.

## Product switch-away candidate

A separate signed candidate stopped during fullscreen setup, before either keyboard chord. The fixture accepted the fullscreen request, but within 2.5 seconds the controller never proved a new SLS current, the fixture member's AX focus and fullscreen state, and that member as the sole occupant in a full CG/SLS inventory. The failed subpredicate was not recorded. Unlike the host-only proof, this candidate focused before switching to the source Desktop and required a whole-Space member census.

The controller retained a blocking recovery receipt. A read-only recheck found the starting currents restored, Mission Control closed, and no fullscreen Space or fixture process, but an extra owned Desktop with offscreen system windows remained. The attempt measured neither the product's redundant refusal nor its switch-away timing. The candidate was removed without changing the existing product refusal or enabling return.

Guarded empty-Desktop removal restored the exact starting topology; the extra Space's system windows disappeared. The only unmatched raw WindowServer row was confirmed at the CoreGraphics cursor-window level and was covered by the existing cursor waiver. The receipt was archived after preserving the readbacks.

## Single product rerun

The single signed rerun reused the host-only entry sequence, including focus after switching to the owned source Desktop. On the entry clock, the first new current SID was sampled at 572,140 µs, its fullscreen type and the exact fixture window's singleton Space membership at 572,455 µs, and the exact AX-focused fullscreen window at 572,936 µs.

The fixture posted the redundant keyboard chord, and exact SLS readback still showed the detached surface current. The controller did not obtain the matching product refusal diagnostic within four seconds, so it did not post the away chord. The missing diagnostic does not prove where product delivery or processing stopped. Final readback found the original currents and order, Mission Control closed, no owned process or Space, and no pending receipt. No product route was promoted.

## Restoration and evidence boundaries

Both completed host runs were the first live exercises of fullscreen-before-shutdown. They retired the detached SID, restored the original currents and ordered topology, and left no owned fixture lifetime or Space and no pending receipt. The retry had only benign offscreen replacements in the controller comparison, with no new residue. Independent native rechecks confirmed restoration; every readable retry pointer sample matched the starting location. Exact Desktop currents and no remaining detached SID rule out a detached Space left current; pixels were not sampled, so visual absence of a black screen is unproven.

The product owner still refuses a detached current before dispatch; its Dock gesture source fence and pinned Mission Control fallback require ordinary user Spaces. Those product refusals do not establish what the host mechanisms can do.

The conservative contract still applies: keep temporary read gaps distinguishable from known absence, require bracketed physical readback around exact foreground observation, and treat a changed SID as supersession even for the same lifetime. These runs do not establish that the host never rekeys a fullscreen surface or guarantees event order. Split View remains unprobed; the fixture has no native Split View affordance.
