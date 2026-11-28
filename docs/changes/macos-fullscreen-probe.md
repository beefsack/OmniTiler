# macOS fullscreen return probe (operational record)

**Status:** operational host evidence only. No product route, wire, or core behavior was enabled. The design record is [the core proposal](macos-fullscreen-return-core-proposal.md).

## Pass 4 signed acceptance

The release gate stays off while the protocol, core and macOS review findings are corrected. Offline regressions must distinguish each defect from the corrected behavior; the protocol/core and macOS suites, workspace compile and baseline Clippy, and KWin typecheck must pass before signing.

The fixture controller must pin the owned home and away Desktops, the exact output, and the detached SID and member lifetime. Under Authentic it must prove both numbered chords are armed before posting, then prove redundant refusal with zero native dispatch, away with exactly one native selection and unchanged fullscreen, and return to the same detached SID and member. No fullscreen-member writes are permitted during those legs. Stop at the first failed proof; one restart is permitted after a proved external disturbance and verified restoration. Promote only after all three legs pass. Verify starting currents and order, pointer, Mission Control, and absence of owned processes and any pending receipt after teardown.

The protocol round-trip regression rejected the old native occupancy delimiter with `encode-mismatch` and passed with the corrected delimiter. The report-wrapper assertion now includes the contract-required `detachedSurface` in held-row key order; `floating` remains the sole retired key in that test. All 16 tests in `report_optionals` passed.

Core regression coverage distinguishes the remembered member from fact order, ordinary external supersession from successful arrival, detached pre-attempt departure from a read gap, same-reply Desktop adoption, both gesture-barrier refusal lanes, and an already-current shared sibling's cursor guard. The core library suite passed 2,542 tests; the core integration suites passed as well.

The macOS adapter now uses exact typed source/target checks before Mission Control opens and before it presses the Space, with other displays' currents still required to be user Desktops. The adapter honors eligible existing-Desktop selections without a numbered-only restriction and withholds detached attempt evidence when the capability is off.

Offline gate: protocol 399 passed; core 2,862 passed; macOS library 1,867 passed and 3 ignored. Workspace compile-only built 88 test executables. Clippy reported only the existing Windows log-sink and macOS fixture-test warnings; KWin typecheck passed.

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

## Offline diagnosis

The old controller kept at most 64 lines with the candidate's `tiler-macos select ` prefix and converted every read error to an empty list. A replay produced the historical `lines:[]` for empty product output, for the generic `tiler-macos dev select … refused` and runtime/core refusal records, and for an unreadable stderr file. The positive control—the candidate's exact route, phase, anchor and target line—matched. Clean teardown had removed the unfiltered logs and final tap counters, so the historical stop layer cannot be recovered. The fixture's `posted` receipt proves submission, not tap delivery.

The bounded source check found no unconditional fullscreen drop: [input eligibility](../../crates/tiler-macos/src/input_eligibility.rs#L430-L463) admits known fullscreen and the numbered chord uses the same mapping as the fixture; [owner selection](../../crates/tiler-macos/src/select_owner.rs#L164-L202) admits the eligible pinned two-display keyboard route without requiring a user-Desktop current. This is not proof that run-time topology, anchor, or command admission succeeded. The [pre-core refusal](../../crates/tiler-macos/src/runtime_thread.rs#L1345-L1350), [core rejection](../../crates/tiler-macos/src/world_dispatch.rs#L1627-L1636), and candidate owner refusal are emitted by different paths.

The diagnostic repair passed nine targeted offline tests. The repaired harness separates no matching tap delivery, pre-core refusal, core refusal and stderr access failure; a generic owner refusal or near-miss candidate line cannot authorize the away chord. It retains unfiltered product stdout/stderr, separate LaunchServices stderr, and the product's terminal tap counters. This verifies the oracle, not the live route.

Mission Control's host-only away leg reached the exact Desktop SID while AX still named the fullscreen member. This does not block a physical switch-away proof that separately checks exact current and unchanged detached membership; it does block claiming that the Desktop sibling gained foreground.

## Signed product rerun with the repaired oracle

On the one signed SIP-on rerun, the first new current SID was sampled at 565,031 µs, the exact fullscreen type and singleton window membership at 565,147 µs, and exact AX-focused fullscreen at 565,427 µs. The redundant chord was posted and exact SLS readback still showed the same detached current. The controller did not obtain the candidate's redundant-refusal line in its four-second wait, so it never posted the away chord.

The retained stderr read succeeded. Terminal tap counters showed two key deliveries, zero exact chord matches or suppressions, zero input forwarded, and zero select enqueues or refusals. The repaired oracle classified **no matching tap delivery**. It does not identify the two delivered events or explain why neither matched. There was no evidence of a pre-core or core refusal, or of a candidate owner refusal. LaunchServices stderr was empty. The raw product streams and terminal outcome were retained.

The product stopped cleanly. Independent readback found the same currents and ordered Spaces, Mission Control closed, the pointer unchanged, no fixture or product process, and no pending receipt. The only raw CG differences were same-owner, same-layer and same-bounds offscreen window-ID replacements. Neither route was promoted. The experimental product candidate and its candidate-specific oracle were removed from the source tree; no additional live leg was run.

### Tap non-match diagnosis

The terminal outcome reported the Ctrl role, Compatible preset, and 51 active chords. The controller posted a numbered `select-N`, whose fixture binding is the shared Desktop digit keycode with no extra modifier bits. [Compatible filters unshifted Ctrl+digit out of the active tap table](../../crates/tiler-macos/src/input_policy.rs#L132-L148), leaving it to macOS; [the product-table test](../../crates/tiler-macos/src/workspace_input_tests.rs#L986-L1001) identifies the same 51-chord table with no numbered selects and all eight auxiliary relative selects. The accepted Desktop run used a relative select, not the same binding. A physical Ctrl+digit is likewise unarmed under Compatible.

This was a candidate setup and oracle error, not evidence of a fullscreen-specific ingress veto. [The callback counts ungated key deliveries before matching armed entries](../../crates/tiler-macos/src/ffi.rs#L10005-L10023) and does not consult foreground or fullscreen state for the match. The two deliveries were not identified in the retained evidence; independently of their identities, the expected numbered chord had no entry to match. The fixture's submission receipt was not a delivery receipt.

The offline check exposed a separate startup defect: Authentic is the fresh-install default, and its 65-row catalogue exceeded the native tap's 64-bit key-up suppression bookkeeping. The cap was in OmniTiler, not CoreGraphics: registration subscribes to three event types, independent of the number of bindings. The runtime refused startup with no fallback. The earlier [successful signed Authentic Desktop leg](v4-phase3-macos-port.md#native-leg-attempts-2026-11-23) had 63 rows. Maximize brought the table to 64; the `d5631dce` merge brought it to 65 by incorporating workspace-tiling toggle, making it the first affected revision on `main`. The Compatible/Ctrl fullscreen rerun had 51 rows and did not hit the cap.

Commit `12fb030fc0` replaces that mask with per-entry pending state and keeps the empty-table refusal. The targeted offline regression passed: native pre-install preparation admitted both real product tables (65 Authentic and 51 Compatible under Ctrl), and the 65th entry suppressed its key-up independently. In one signed SIP-on startup smoke with fresh settings and no fixture windows or input, the default Ctrl/Authentic session reported 65 armed chords and an enabled Session filter at 206 ms after launch request. The exact-run stop was requested at 210 ms. At 396 ms, the run reported clean shutdown with no input refusal or deliveries, its process had exited, and an independent CoreGraphics tap inventory found no remaining tap for its PID.

### Armed-chord rerun: native owner refused before dispatch

The candidate's updated oracle passed 17 targeted offline tests. It checks both exact numbered bindings from the running tap table before posting, distinguishes `chord-not-armed` from missing delivery, and retains the fixture's submitted key identity separately from aggregate tap counters. The tap does not log raw user input.

In one signed SIP-on rerun, the first sample of the exact detached current was at 580,434 µs, its fullscreen type and singleton window membership at 580,564 µs, and the exact AX-focused member at 584,239 µs. The Authentic/Ctrl product reported 65 armed chords, including both required numbered selects. The fixture posted `select-2`; the terminal tap counted two deliveries, two exact matches and suppressions, and one select enqueue. The operation reached the native select owner, which refused with `fresh row unprovable (workspace row/current identity unprovable)`, not the candidate's redundant-target refusal. The detached current stayed unchanged, and the controller did not post the away chord.

The ordinary [user-Space snapshot](../../crates/tiler-macos/src/workspace_select.rs#L80-L98) excludes typed fullscreen Spaces from its Desktop row but retains the native current SID. Its [validation](../../crates/tiler-macos/src/workspace_select.rs#L21-L36) rejects a current SID outside that row, and the [ordinary owner refuses](../../crates/tiler-macos/src/workspace_select.rs#L922-L934) before a native dispatch decision about the target. This guard applies to both redundant and away targets while the same detached current persists. The candidate could bypass it only with separate admission proof; the generic refusal does not identify which prerequisite failed or show how an unposted away chord would have behaved in the candidate. Carry detached current explicitly through the [Pass 3 observation contract](macos-fullscreen-return-core-proposal.md), rather than treating the covered Desktop as current.

Independent teardown readback found the original currents and ordered Spaces, closed Mission Control, the same pointer, no owned processes, and no pending receipt. The only CG differences were offscreen window-ID replacements with unchanged owner, layer, and bounds. The candidate and its oracle remain out of the product tree; neither fullscreen route was promoted.

## Restoration and evidence boundaries

Both completed host runs were the first live exercises of fullscreen-before-shutdown. They retired the detached SID, restored the original currents and ordered topology, and left no owned fixture lifetime or Space and no pending receipt. The retry had only benign offscreen replacements in the controller comparison, with no new residue. Independent native rechecks confirmed restoration; every readable retry pointer sample matched the starting location. Exact Desktop currents and no remaining detached SID rule out a detached Space left current; pixels were not sampled, so visual absence of a black screen is unproven.

The product owner still refuses a detached current before dispatch; its Dock gesture source fence and pinned Mission Control fallback require ordinary user Spaces. Those product refusals do not establish what the host mechanisms can do.

The conservative contract still applies: keep temporary read gaps distinguishable from known absence, require bracketed physical readback around exact foreground observation, and treat a changed SID as supersession even for the same lifetime. These runs do not establish that the host never rekeys a fullscreen surface or guarantees event order. Split View remains unprobed; the fixture has no native Split View affordance.
