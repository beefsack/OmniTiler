# macOS fullscreen return probe (operational record)

**Status:** operational host evidence only. No product route, wire, or core behavior was enabled. The design record is [the core proposal](macos-fullscreen-return-core-proposal.md).

## Pass 4 signed acceptance

The release gate stays off while the protocol, core and macOS review findings are corrected. Offline regressions must distinguish each defect from the corrected behavior; the protocol/core and macOS suites, workspace compile and baseline Clippy, and KWin typecheck must pass before signing.

The revision fence now previews the exact requested workspace without advancing history. It rereads only a stale remembered member for that target and refuses before adoption if the owner proof or the ticket-correlated detached View does not match. The six adapter regression failures were fixture gaps: the test omitted window metadata that the real macOS adapter supplies, causing the scope to be held before focus could establish Home memory. With that metadata, a native windowed focus observation seeds Home before fullscreen entry without a typed Focus command. All seven fence tests passed, including no probe for fresh evidence and refusal without a Home dispatch. The gate-off revision passed 399 protocol tests, 2,868 core tests, 1,962 macOS library tests (3 ignored), and the KWin typecheck.

The fixture controller must pin the owned Home and Away Desktops, exact output, and detached SID and member lifetime. Under Authentic it must prove both numbered chords are armed, then prove redundant refusal with zero native dispatch, Away with exactly one native selection and unchanged fullscreen, and Return to the same detached SID and member with native focus. The legs must not write to the fullscreen member. Stop at the first failed proof; diagnose and fix reproducible failures offline before another attempt. Promote only after all three legs pass in one run. Teardown must verify starting currents and order, the pointer, closed Mission Control, and absence of owned processes, Spaces and pending receipts.

The second independent review identified two product blockers: select-time detached membership depended on an off-Space `AXWindows` join, and the two-display owner refused the core's exact-current marker because that marker intentionally carries no View flight. It also identified deterministic failures in the candidate's occupancy, AX, Space-order, dispatch-log and cleanup oracles. Before another product run, use a separate signed fixture-only capture of a native round trip with no product or chord posts. Use those snapshots to test the product corrections and a replacement acceptance harness; diagnostic chrome rows, HID deltas and incidental focus readings are not acceptance gates. Keep the production capability off until a single clean run proves all three legs.

The first dedicated signed capture stopped during setup, before fullscreen or a Mission Control round trip. Baseline was captured at 0 ms, the home Desktop and its fixture window at 5,115 ms, and the teardown read at 8,435 ms (10.24 s wrapper wall time). No product launched and no shortcut was posted. The home window had singleton Desktop membership and was the only raw AX window; eight other fixture CG rows were hidden layer-zero auxiliaries. The retained off-Space element, fullscreen AX inventory, fullscreen CG geometry and membership, and post-switch focus/HID contrasts were not sampled. Recorded HID deltas through the stop were zero.

The second Desktop setup called the return probe again. The inner create had verified one addition against a fresh row, but the outer probe compared against the leg's pre-home row and counted both the already-owned home Desktop and the temporary addition. The refusal does not establish an insertion index. The temporary addition was removed. The controller restored both original currents and the pointer and reaped the fixture and awake helper, but the home Desktop remained: its teardown gate counted an offscreen WindowManager row as an occupant. Mission Control closure was not verified and the recovery receipt stayed active.

The receipt-bound recovery removed only that inactive Desktop after rechecking every window membership and authenticating its five offscreen system rows. Removal and independent reads confirmed the original Space order and currents, verified Mission Control closure, and found the original pointer unchanged. The first recovery comparison falsely reported a mode change: the standalone AppKit read was cold and returned `false`, while the original fixture baseline had initialized AppKit. The same native getter now initializes AppKit before both reads, without activation or a settings write.

A fresh signed verify-only run read the expected separate-Spaces mode, verified Mission Control closed, and matched the original baseline with no non-benign differences. The pointer independently matched, and no leg-owned process or socket remained. Recovery evidence and the residue were preserved before archiving the baseline receipt; no active receipt remained.

The second fixture-only capture reached native fullscreen in 11.13 s. The fullscreen Space appeared between the two owned Desktops without reordering the existing row. The retained window handle still read the original window ID and fullscreen state, but raw `AXWindows` omitted it and instead listed a different, on-screen layer-zero window from the same owner. The actual fullscreen window was marked offscreen in CG despite live, nondegenerate geometry. Focus sources did not agree on the fullscreen member. The capture stopped on the Mission Control presence wait before switching away, so it did not establish off-Space handle validity, post-switch geometry or focus. Recorded HID deltas were zero.

Teardown exited fullscreen, reaped the fixture, and restored the pointer and original currents. Both owned Desktops remained: the typed emptiness check refused the shared Finder desktop-icon plane because it was on screen. The receipt recovery now strips the complete recorded created-SID set when proving original order, while still requiring fresh proof for each individual inactive target. Only the Finder desktop-icon layer with an exact Finder identity is exempted from the on-screen check; ordinary Finder windows and unknown owners still block.

Signed recovery removed the two recorded Desktops one at a time. An independent readback after each removal verified the precise ordered row, original currents and closed Mission Control. The final verify-only read matched the baseline with no non-benign differences. The pointer matched independently; no owned processes or sockets remained, and both recovery receipts were archived.

The second capture's post-run open Mission Control was not evidence of a delayed fullscreen-source launch: the reporter itself called the mutating open helper before and after teardown. The original 2 s wait had no contemporaneous CG presentation read. The fixture-only follow-up uses the existing 8 s action timeout, samples AX group and WindowManager presentation independently, and preserves both first-positive timestamps. Evidence serialization consumes read-only observations, and fullscreen teardown will not remove a Desktop without verified absence.

The final signed fixture-only capture completed in 18.63 s. WindowManager's presentation rows first read positive 60 ms after launch; the AX group first read positive at 307 ms. AX alone authorized the press. Neither the earlier 2 s timeout nor a persistent fullscreen presence mismatch reproduced, so the second capture does not establish the cause of that stop.

The native round trip proved that the same fullscreen Space and window survived Away and Return. The ordered row retained the fullscreen Space between Home and Away. While away, raw `AXWindows` reported only the Away window; the retained fullscreen handle still returned the original window ID, fullscreen state, minimized state and nondegenerate frame, and the exact owner's offscreen CG window retained singleton fullscreen membership. The initial full-size `AXUnknown` auxiliary disappeared by Away and did not recur on Return. A separate short layer-zero same-owner surface was on screen alongside the true window after Return. A blanket on-screen layer-zero exclusivity check would therefore refuse a valid return; any exception must be specific to authenticated auxiliary identity and shape, not to layer or on-screen status alone.

The post-Mission-Control focus read observed the Away sibling before any explicit focus action, then the original fullscreen window on Return. HID deltas remained zero. Independent readbacks matched the initial Space order, currents and pointer and confirmed closed Mission Control. No owned process, socket, Desktop or active receipt remained. The transaction was fixture-only: the product gate remains off and none of the three gated acceptance legs have passed.

The replacement acceptance gate distinguishes the exact live fullscreen anchor from auxiliary rows: it requires the anchor's singleton membership on a fullscreen Space, permits same-owner layer-zero rows, refuses any foreign on-screen layer-zero member, and excludes negative layers from the decision. The Return oracle requires the original window to gain native focus without a fixture focus command; setup focus is not evidence of arrival.

The protocol round-trip regression rejected the old native occupancy delimiter with `encode-mismatch` and passed with the corrected delimiter. The report-wrapper assertion now includes the contract-required `detachedSurface` in held-row key order; `floating` remains the sole retired key in that test. All 16 tests in `report_optionals` passed.

Core regression coverage distinguishes the remembered member from fact order, ordinary external supersession from successful arrival, detached pre-attempt departure from a read gap, same-reply Desktop adoption, both gesture-barrier refusal lanes, and an already-current shared sibling's cursor guard. The core library suite passed 2,542 tests; the core integration suites passed as well.

The macOS adapter now uses exact typed source/target checks before Mission Control opens and before it presses the Space, with other displays' currents still required to be user Desktops. The adapter honors eligible existing-Desktop selections without a numbered-only restriction and withholds detached attempt evidence when the capability is off.

Offline gate: protocol 399 passed; core 2,862 passed; macOS library 1,867 passed and 3 ignored. Workspace compile-only built 88 test executables. Clippy reported only the existing Windows log-sink and macOS fixture-test warnings; KWin typecheck passed.

The candidate passed 1,894 macOS library tests (3 ignored); all-target Clippy matched the existing baseline. Both candidate bundles built and passed independent signature verification: product 216 s, fixture 144 s, verification 1.36 s. The signed read-only preflight confirmed Accessibility, event posting, an unlocked session without Secure Input, and readable topology and pointer with Mission Control closed.

The signed Authentic sequence stopped in `detached-away-prepare` after 10 s overall. It created the owned home Desktop and joined the subject, but the oracle counted an all-Spaces desktop window and three additional unclassified rows in its raw membership inventory. That inventory cannot prove the fixture predicate: a fresh owned Desktop may still contain positively identified system surfaces. This was a harness failure, not a product refusal or an input disturbance. No retry was taken. The product tap did not launch, fullscreen setup did not run, no chord was posted, and no fullscreen member was written.

| Leg | Result | Timing |
| --- | --- | --- |
| Redundant | Not reached; exact running tap bindings and native dispatch not sampled | — |
| Away | Not reached | — |
| Return | Not reached | — |

Controller teardown was clean. Independent readback matched both displays' starting currents and ordered Spaces, the starting pointer without a warp, and closed Mission Control; no owned process, owned Space, or recovery receipt remained. Neither route was enabled or advertised. The candidate was not merged: the membership oracle needs an authenticated caller/window join and must distinguish proven system/fixture auxiliaries from other user windows, refusing unreadable or ambiguous membership.

The repaired membership audit authenticated the owned fixture caller and counted only positively identified non-user surfaces as exclusions. The first staged run excluded the native surfaces but refused the expected fixture row because the aggregate application inventory had not observed the new process. A direct per-PID read succeeded. The candidate now checks that direct bundle, executable and launch token against the registered fixture lifetime, without widening any exclusion. The candidate's 37 focused tests and full macOS library suite passed (1,911 passed, 3 ignored).

In the next signed Authentic sequence, both owned Desktop audits passed before product launch, with exactly the joined fixture caller and positively identified WindowServer menu bars, WindowManager desktop and Finder desktop. A preceding invocation supplied the product instead of the fixture designated requirement and stopped at signature validation without posting; teardown was clean.

That sequence stopped after 13.11 s at the entry fixture-window census, before exact binding verification, a fullscreen request, or any chord. The census counted eight extra fixture-owned, layer-zero CG rows. WindowServer recorded them as hidden from creation with no Space joins; neither owned Desktop audit contained them. The PID-and-layer census counted auxiliary CG entries instead of the exact joined AX callers. This was a harness refusal, not a product-route outcome. Product tap counters recorded zero deliveries, matches, suppressions or select enqueues. Redundant, away and return were not reached; no leg timing or fullscreen-member write was recorded.

Independent readback matched the original ordered Spaces, both currents and the pointer without a warp, found Mission Control closed, and found no owned Space, process or receipt. The default-disabled signed product was rebuilt and verified. No capability was promoted.

The next candidate replaced the PID-and-layer census with a signed-lifetime AX caller census and a one-to-one CG and singleton-SLS join. It did not widen any whole-Space exclusion. Seven targeted tests and 1,918 macOS library tests passed (3 ignored). Workspace all-target Clippy matched the existing baseline.

The signed Authentic run with that candidate passed both owned Desktop audits and no longer counted hidden fixture CG auxiliaries as callers. The entry census then refused: with home current, the fixture's `AXWindows` returned the home caller but omitted the away caller, despite the latter's earlier authenticated AX join while away was current, unique CG row, and exact singleton away membership. The run stopped before exact binding verification, fullscreen, or any chord. No acceptance leg ran and no product route was measured.

Independent readback again found the original ordered Spaces and currents, unchanged pointer, closed Mission Control, and no owned native state or process. The controller retained a receipt because its new census evidence file was not registered as an owned evidence leaf. The receipt was archived after verified restoration and evidence preservation; no pending receipt remained. The default-disabled artifact was rebuilt and verified. No retry or promotion followed. The candidate remains unmerged: the harness needs a phase-aware caller proof that does not require simultaneous AX enumeration across Spaces, and it must register or relocate its census evidence file.

## Replacement harness: signed Authentic acceptance

The gate-off replacement harness validates the registered fixture member and pinned topology through dispatch, and requires proven product absence and fullscreen retirement before recovery can remove an owned Desktop. A fallback exit can restore the session but cannot count as acceptance. The offline gate passed 399 protocol tests, 2,868 core tests, and 1,924 macOS library tests (3 ignored), plus KWin typecheck. All-target Clippy matched the existing baseline.

With only the capability flipped for the signed candidate, the first run verified the two required Authentic bindings, proved fullscreen entry at 619 ms, and registered the same window at 624 ms on the controller clock. The wrapper took 13.39 s.

| Leg | Result | Timing |
| --- | --- | --- |
| Redundant | Stopped before native dispatch. The product refused selection admission because fresh row, display legs, or select wake were unpublished. That is not the required operation-correlated refusal for the already-current destination. | 318 ms from leg start to stop |
| Away | Not reached | — |
| Return | Not reached | — |

The tap reported two deliveries and two suppressions. Immediately before the refusal, the runtime reported incomplete window inventory; the capture does not identify which admission predicate failed. The stop did not establish any external disturbance. The controller proved retirement of the exact fullscreen Space and clean product shutdown. Independent readbacks matched the starting order, currents, and pointer, confirmed Mission Control closed, and found no owned process, Space, socket, or pending receipt.

The native refresh records CG/SLS evidence for a retained member omitted by `AXWindows`, but did not read the retained AX handle on that path. Without a prior fullscreen observation, CG/SLS evidence alone cannot refresh the detached member. This matches the fixture-only capture, where `AXWindows` listed the auxiliary at entry but the retained handle could read the exact fullscreen member. The failed product observation did not log the member's singleton SLS Space or its type. The candidate was replaced with the verified default-off build; neither route was enabled or advertised.

The repair reuses the selection owner's read-only proof for a known, retained window that `AXWindows` omits. It requires the exact owner and window, a positive AX fullscreen read with minimized false, a live CG row, and matching singleton SLS membership on the typed fullscreen Space. A refusal keeps the hold and cannot supply a Desktop home. The absence trace now records the typed singleton and the retained-read outcome so another refusal can distinguish the two.

The second signed attempt proved fullscreen entry at 653 ms and registered the member at 658 ms. The wrapper took about 14 s by wall clock. The new trace observed `retained=proved` on the typed fullscreen Space while `AXWindows` omitted the member.

| Leg | Result | Timing |
| --- | --- | --- |
| Redundant | The product emitted the exact operation-correlated refusal with zero native dispatch. The controller stopped because it counted the preceding submission and observation records as extra terminals. | 607 ms from leg start to stop |
| Away | Not reached | — |
| Return | Not reached | — |

The controller did not reach its completed-span no-write audit. It proved fullscreen retirement and clean shutdown. Independent readbacks matched the starting Space order, currents, and pointer, confirmed Mission Control closed, and found no owned process, Space, socket, or pending receipt. No external disturbance was proved.

Replaying the captured three records reproduced the controller refusal offline. The classifier now excludes the two nonterminal progress record forms and still requires zero dispatch and exactly one refusal with a nonzero operation ID, the exact surface and member, and the exact no-dispatch reason. A duplicate refusal, a different target, or any dispatch fails the replay.

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
