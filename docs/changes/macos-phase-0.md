# macOS Phase 0 — decisions and capability probes

## Goal and acceptance

Establish the macOS architecture decisions, live-testing/recovery protocol,
persistent development signing, and an all-Rust capability-probe application.
Build and check the portable and macOS boundaries; distinguish live evidence,
source-derived expectations, permission blockers, and untested behavior.

Acceptance covers AX window identity/observers/frame/focus/timeout probes,
registered hotkeys and bounded event taps, native Spaces capabilities with SIP
enabled, cleanup/restoration, and documented next steps for blocked operations.
No Dock injection or reduced SIP; no release/notarization or tiling product UI.

## Approach

1. Research the support floor; record the adopted language, thread topology,
   workspace, modifier, architecture, and signing decisions. Establish the
   macOS live-testing protocol before live experiments.
2. Implement a platform-gated Rust probe, persistent signed app-bundle recipe,
   and appropriate CI checks. Review live effects and recovery before launch.
3. Run bounded probes, restore owned changes, check cleanup, and record evidence.
4. Independently review live behavior and signing; reconcile concrete findings.

## Verification

Use mise-managed Cargo/just, formatting, strict clippy, portable-crate tests,
signature/requirement checks, and bounded live runs. API success alone is not
behavior proof. Preserve pre-existing working-tree changes.

## Adopted decisions

Recorded in `docs/decisions.md` and the plan's adopted/open table: all Rust,
[Unsafe Code](../principles.md#unsafe-code); AppKit main/input/per-app AX/
engine threads with channels; native Spaces first and yabai primary reference;
configurable Meta defaulting to Ctrl; Apple Silicon only; persistent self-signed
development identity; macOS 15 minimum. Homebrew's current Mac-only events put
15+ at 93.27% (developer/event-weighted proxy, not population proof);
StatCounter's modern-macOS reporting cannot resolve version coverage. Sources,
denominators, and private-API risk rationale are in the durable decision.

## Implemented milestone

- Live protocol and root guidance pointer; setup instructions for Certificate
  Assistant, stable signing, scoped TCC, and recovery.
- `tiler-macos` with platform stubs, owned AppKit helper, bounded input and AX
  probes, authenticated process records, SLS read-only diagnostics. Default
  preflight never opens windows or installs hooks. Fixed-ID/path signed-bundle
  recipe fails closed without one unique persistent signer.
- AX workers construct/use/drop native handles on their app thread; bounded
  engine queue; minimal event-tap callback; Carbon uses main-thread event
  dispatch. No custom unsafe Send/Sync implementations or cross-thread AX
  pointers. Plain Cargo builds do not enforce the bundle's deployment target;
  bundle recipe exports 15.0 and verifies Mach-O load commands.
- Initial simulated AX/input paths were rejected and replaced by genuine API
  calls. Review corrections added exact-chord opt-in suppression, bound nonce/
  executable/start identity, ready handshake, focus-preserving window creation,
  exact signing-identity matching, output ownership marker and symlink guards.
  Corrected paths pass the checks below; live effects remain unverified.

## Initial verification (2026-10-14; superseded below where noted)

Native graphical-session metadata queries with SIP enabled; no host release,
window titles, display UUIDs, local paths, or raw input recorded here.

| Capability | Status | Evidence |
| --- | --- | --- |
| Default/preflight | Works | `mutates:false`, private SLS symbols available, stable bundle ID reported |
| App enumeration / AX trust | Permission-blocked | 45 apps enumerated; `process_trusted:false`, 45 explicit AX failures, zero windows read; zero is not evidence of no windows |
| Secure Input observation | Works (current observation only) | `Inactive`; active-state blackout/recovery not tested |
| Displays / Spaces / current Space | Works for initial topology | One display and one Space; current Space equals the sole enumerated member; real SkyLight readback |
| Window-to-Space membership | Implemented, untested | `SLSCopySpacesForWindows` read path; no owned window ID tested; zero ID refused |
| AX join / observers / frame / raise / hung-app behavior | Live-blocked | Implemented paths; no stable signed identity or AX grant, so no window-control evidence |
| Carbon registration/delivery | Untested | Real bounded registration/handler path; neither live registration nor physical delivery attempted |
| CGEventTap suppression / timeout-disable recovery | Untested | Real bounded tap, opt-in exact-chord suppression and re-enable path; pure matching tests do not establish physical input behavior |
| Native Space switch / move / create / destroy | Unimplemented, untested | Mutating Space APIs not called; move command explicitly unsupported. This is not evidence of SIP rejecting an API |
| Persistent signing / TCC rebuild survival | Signing works; grant survival pending | Superseded by follow-up below: strict verification and stable requirement across two clean rebuilds; Accessibility grant not yet present |
| Cleanup | Clean | No probe/helper process remains; no tap/hotkey/window/Space mutation was run, so no restoration preimage was exercised |

Read-only commands completed promptly (sub-second after build); no detailed
latency distributions or action/restore latencies have been measured.

Passing commands after thread/identity/signing corrections:

```sh
mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-macos
mise exec -- cargo test --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-macos
mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-macos --all-targets -- -D warnings
mise exec -- cargo fmt --all -- --check
bash -n scripts/macos-app.sh
mise exec -- just --list
git diff --check
```

`tiler-macos` has 51 unit and 5 integration tests passing on macOS. CI is
configured for the native macOS allowlist, Windows stub, and Linux workspace;
Linux/Windows job outcomes have not been observed. License expressions were
inspected (all offer existing allowed alternatives), but cargo-deny/about
tools are unavailable locally; their CI gates remain pending. Notice shipping
roots currently cover Linux products, not this development probe.

## Current milestone and evidence (2026-10-14)

### Persistent signing follow-up

Bounded acceptance: accept one exact-name code-signing identity regardless of
trust, sign by its SHA-1, retain missing/ambiguous identity and bundle guards,
and compare fixed bundle ID/designated requirement across two clean rebuilds.
Verify both bundles strictly, run only the nonmutating bundled preflight, and
repeat local formatting, strict clippy, tests and macOS allowlist build. Align
the macOS unsafe decision references with the project principle. Accessibility
grant survival and the remaining live capability legs are separate acceptance.

Completed: the recipe scans all matching code-signing identities, ignoring the
repeated valid-only subset, and signs by the unique exact-name match's SHA-1.
Six mocked-identity runs of the actual script passed: untrusted, trusted/repeated,
missing, substring-only, duplicate and missing-hash cases. Existing fixed-output,
owned-marker and symlink guards remain. Actual assembly exposed two existing
packaging failures: BSD `chmod` rejects `--`, and codesign rejects the marker
directly under `Contents`. Removing that separator for fixed absolute paths and
placing the marker under `Contents/Resources` resolved both.

Two clean rebuilds (`cargo clean -p tiler-macos` before each `just app`), each
followed by `just verify` and explicit `codesign --verify --strict`, passed.
Both nested executables retain minos 15.0. Bundle ID
`io.github.omnitiler.macos.probe` and the designated requirement were identical
across builds; the requirement is the identifier plus the persistent certificate
leaf hash (hash omitted here). The signer remained untrusted; no trust change
was needed. `spctl` rejected the unnotarized development bundle as expected.

Bundled `preflight` exited normally with `ax_trusted:false`,
`post_event_access:false`, `mutates:false`, SLS symbols available and Secure Input
Inactive. No probe remains running. This establishes persistent signed identity,
not Accessibility consent or consent survival. Formatting, strict clippy,
allowlist tests/build (`--all-targets` for clippy/build), shell syntax and diff
checks pass. Unsafe policy references now link to the unchanged principle.

### Maintained bindings and recovery

- Public AX/CF declarations replaced with `objc2-application-services 0.3.2`
  and `objc2-core-foundation 0.3.2`; existing `objc2` AppKit/Foundation and
  `core-graphics`/`core-foundation` cover AppKit, CGWindowList and taps.
  `objc2-core-graphics 0.3.2` supplies the non-prompting public
  `CGPreflightPostEventAccess` binding (direct macOS-only dependency;
  already locked transitively, no additional crates).
  Existing `libc` covers loader/process calls. New crates offer MIT within
  `deny.toml`; cargo-deny itself remains unavailable locally.
- Hand-written extern functions/statics: **36 + 2 → 1 + 0**. The sole extern
  declaration is `_AXUIElementGetWindow`; uncovered Carbon/SkyLight APIs
  still have handwritten dynamic function-pointer signatures. The declaration
  surface is smaller; the approximately 2,000-line safe-wrapper/stub boundary
  is not yet tiny. No custom unsafe Send/Sync; sibling modules forbid unsafe
  (crate-root deny permits the single FFI exception).
- Maintained AX bindings still require isolated unsafe wrapper calls. Earlier
  textual unsafe counts measured the bindings-only milestone, not the current
  wrapper surface.
- Added authenticated owned CG window enumeration, direct-only native move
  and separate restore commands, exact single-Space effect comparison, CG
  baseline capture, and structured post-dispatch failures retaining preimages.
  Spawn executable/nonce and fresh WID-to-PID association are checked
  again at write/readback boundaries; checks narrow races rather than eliminate
  them. AX workers also revalidate the spawn record before writes/restoration.
- Review rejected the compat-ID fallback: `SLSSpaceSetCompatID` has no proven
  exact preimage/readback, and resetting to zero is not exact restoration.
  Its callable setters were removed; availability is diagnostic only. Direct
  moves remain untested; `--allow-untested` is not proof of independent recovery.
- AX frame transaction now restores after every possible write, including
  position-accepted/size-refused and readback failures, when fresh ownership
  permits. Failure JSON retains preimage, attempted/applied frame, restore
  readback and failure/refusal reasons. Restoration requires exact readback,
  never API acceptance. Fault-injection tests cover partial write, failed set,
  readback, restore write, restore mismatch and ownership loss. No AX live
  acceptance is claimed. The app and owned window have finite messaging
  timeouts; the frame caller budget is 75 seconds, including nonce-title
  revalidation. Timeout means outcome unknown, possibly in-flight restoration.
- Independent review corrections retain structured restore-write errors,
  bound enumeration to one 3-second aggregate deadline and at most 64 apps,
  add fresh CG joins to frame boundaries, and retain cleanup/residue evidence
  on failed holder runs. Unknown wait/CG results never mean verified cleanup.

### Earlier signing attempt (superseded by persistent signing follow-up)

System OpenSSL generated a disposable self-signed codeSigning-EKU identity;
`security import` with codesign-only access succeeded. Certificate verification
reported `CSSMERR_TP_NOT_TRUSTED` / `AnchorTrusted=0`; there were still zero
valid code-signing identities. A disposable codesign operation timed out at
the key-access authorization boundary. The imported identity and private-key
material were removed; zero valid identities and unchanged trust settings were
confirmed, with no timed-out codesign process remaining. No persistent signed
bundle, designated-requirement comparison or TCC-rebuild-survival evidence
existed at that point. The bundle recipe retains its missing/ambiguous-signer
refusal. The later persistent signing follow-up above supersedes this blocker;
valid-only identity enumeration was not necessary for signing.

### Owned-helper fixture: blocker resolved

Earlier direct `Child` launches reached the identity/ready handshake but could
not acquire the `NSRunningApplication.launchDate` start token. Bare executable
and temporary bundle arrangements failed with:

```text
probe helper start identity unavailable; refusing recordless session
```

A helper-as-main temporary bundle registered the correct live bundle/executable,
but `launchDate` was nil. One causal `finishLaunching()` repair produced the same
bounded refusal and was reverted. These failed arrangements are superseded,
not retries to pursue. The launch token has been removed, without adding
process-start identity machinery:

- `spawn_verified` holds the exact Child and verifies its PID, bundle constant
  and random per-run nonce returned through that child's stdout, followed by
  a ready handshake. The helper also carries that nonce in its window title.
- `spaces-session` retains the Child through every read and cleanup, checking
  liveness, executable and a fresh CG owner-PID/window association. No title
  permission is needed for this holder path.
- Separate commands authenticate record/executable/PID plus a **current**
  title nonce. Caller-record equality alone was rejected on review: a stale
  record and same-executable PID reuse would otherwise pass. CG withheld the
  owned window's title, so that channel fails closed. A bounded, per-app AX
  title fallback is implemented for observed Accessibility trust; its positive
  leg is untested. No Screen Recording dependency is introduced.
- Exact-Child graceful SIGTERM, kill fallback, bounded reap and record removal
  are proven live. Error branches retain structured cleanup evidence; records
  are removed only after verified exit. No raw PID signal follows a reaped
  Child. Cross-command `probe-stop` remains title/consent-blocked.

One short-lifetime spawn initially expired during opportunistic launch-token
polling; removing the token and its polling removes that cause. A provisional
title-less cross-command check is superseded by live nonce authentication.
Earlier residue checks used the wrong temporary-directory assumption; current
checks use the actual runtime temporary directory and verify it is empty.

### Current live capability results

Physical graphical-session probes with SIP enabled. Final holder run: owned
PID `59274`, CG window `598`, sole Space `1`, membership `[1]` before/after,
current `[1]` before/after. Graceful and kill proofs both report exit/reap/record
removal/process-gone true; final stop is graceful with no kill fallback and
`cg_residue:false`. Runtime record directory is empty; no helper/probe remains.
No Space was created, moved, switched or destroyed. AX trust and non-prompting
post-event-access preflight are both **false**; Secure Input observed Inactive.
Mission Control automation and gesture routes were not dispatched. Their
permission shortfall is not an OS Space-call failure or SIP rejection.

| Capability | Result | Evidence | Pinned yabai SA dependency |
| --- | --- | --- | --- |
| Display/Space/current enumeration | Works for observed topology | One display, one Space, current equals member before/after helper attempts | None |
| Owned CG ID → Space membership | **Works via Child holder** | CG owner PID equals Child PID; `SLSCopySpacesForWindows(cid, 0x7, [wid])` reads `[1]` before/after | None |
| Owned move/back: direct SLS | Skipped, untested | `SLSMoveWindowsToManagedSpace` symbol present; one Space, independent restore unproven; no dispatch | None for direct route; version/workaround branch differs |
| Owned move/back: bridged asynchronous SLS operation | Unavailable on observed runtime; effect untested | `SLSPerformAsynchronousBridgedWindowManagementOperation` symbol absent | None |
| Owned move/back: compat ID + workspace setter | Skipped, untested | Symbols present; callable setter disabled because exact compat-ID preimage/restore is unproven | Non-SA fallback after SA failure in workaround branch |
| Process → Space assignment | Source-only, untested | `SLSProcessAssignToSpace` identified; no callable probe or OS attempt | None |
| Switch/focus: Dock-swipe gesture fallback | Permission/recovery-blocked, untested | Post-event access false, one Space; no synthetic event or switch readback | None; yabai focus falls back after SA failure |
| Same-display switch / cross-display swap | Source-only, untested | No switch executed; one display/Space; swap also needs owned-move recovery | Same-display switch uses SA; cross-display swap uses move routes |
| Create/destroy: Mission Control UI | Permission-blocked, untested | AX trust false and post-event access false; no temporary Space or UI automation | None for UI; yabai create/destroy commands use SA |
| AX / Carbon / intercepting tap | Blocked/untested | AX trust false; no hooks installed; persistent signing now verified in follow-up above | Not applicable |
| Cleanup | **Works for Child-held probes** | Exact child gone/reaped, records empty, no owned CG residue, layout/current identical | Not applicable |

Source basis (local pinned-source inspection, not live efficacy):
[yabai move](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705),
[switch/focus](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035),
[create/destroy](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1037-L1072),
[ABI](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h).
[window-manager send path](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L2092-L2121),
[process assignment](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L1074-L1077),
and [SA focus/create/destroy dispatch](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/sa.m#L442-L482)
complete the route classification.
No capability is labeled blocked-by-SIP without an actual OS attempt/readback.

### Final verification

All initial command gates above pass on the latest implementation; build uses
`--all-targets`. `tiler-macos`: **95 unit + 9 integration** tests pass; the
portable/macOS allowlist totals **949 passing tests**. Host
`cargo check --locked -p tiler-windows` passes (not native Windows evidence).
Only the native macOS target is installed; Windows/Linux native checks are not
locally available. No Linux/Windows CI outcome has been observed; macOS-floor
runtime and cargo-deny CI remain pending. New bindings remain macOS target-gated.

Source basis: revision `807ed9799d99a1e77fdded46e52f9d4981195eec` plus the
current uncommitted probe/binding/recovery changes. Final build-tree SHA-256:

- `tiler-macos`: `129c248f5e43c51c67c26b5642ea65e2e2e51b36f04b6a0c090fc8b4a2c6f2b7`
- `tiler-macos-probe-target`: `a913734c04a0fa15d6eee91ffa99a81593ef393859280282412e1bcf49ce2e89`

These are linker-ad-hoc artifacts, not persistent TCC identity. After the final
diagnostic-text cleanup, allowlist build/strict clippy/fmt and all macOS tests
pass again; the holder run and residue checks above use these rebuilt artifacts.
Raw operational diagnostics remain outside repository records.

## Remaining acceptance and exact continuation

This is an incomplete Phase 0 milestone, not release or live-control acceptance.

1. **Exact next action — SLPS focus fixture:** after an unlocked graphical
   session is available, rebuild/sign/verify and require LaunchServices
   `ax_trusted:true`; establish agreement on the original frontmost app, then
   run the pending `focus-session --seconds 30` fixture. Require an actual
   original → target → original transfer using the SLPS route, with NSWorkspace,
   AX PID/window and CG ownership agreement at each boundary, including after
   helper exit. See the latest investigation below. Only then implement/run
   per-event-gated Ctrl+h Carbon delivery, exact tap suppression, passthrough,
   post-release delivery, actual disabled-tap recovery and an independent hard
   lifetime ceiling. Read enabled Mission Control Ctrl+arrow/digit bindings
   before collision probes; registration success alone is not collision
   behavior. Physical acceptance remains separate. Input Monitoring is needed
   only if the selected mode proves it needs it.
2. Space writes require a second disposable Space and independently demonstrated
   restoration surviving primary exit. The same-holder readback is **not** that
   proof; `spaces-session` never dispatches a write, even with `--allow-untested`.
   Establish signed/trusted UI automation and separate restore first, then use
   Mission Control to create/identify a temporary Space, probe each safe non-SA
   move/back and switch/back route with membership/current readbacks, and remove
   only that created Space. Preserve original layout/current. Compat route still
   needs an exact preimage solution; unsupported outcomes remain TBD. No SIP
   failure may be inferred from symbol presence, no dispatch or missing topology.

Existing kickoff evidence remains in `archive/macos-port-kickoff.md`.

## Live AX milestone (2026-10-14; supersedes earlier permission blockers)

Physical graphical-session probes, SIP enabled. Fixed signed bundle
`io.github.omnitiler.macos.probe`, persistent signer, identical designated
requirement across rebuilds (leaf hash omitted). LaunchServices preflight reports
`ax_trusted:true`, `post_event_access:true`, Secure Input Inactive and SLS symbols
present. The grant survived repeated genuine rebuilds. Direct executable launch
reports trust false because the terminal is the responsible process; it is not
evidence of missing bundle consent. `open -W` failed; `just preflight-app` and
`just launch` use bounded polling. Wrapper exit is not the app's exit status;
JSON readbacks gate capability acceptance.

| Capability | Verdict | Observed evidence and latency |
| --- | --- | --- |
| Enumeration / private AX-to-CG join | Works on owned fixture | Trusted enumeration returned windows; AX WID equals fresh CG owner-PID/WID; final holder join 9ms |
| AX-title nonce / separate stop | Works | CG title withheld, positive AX fallback authenticates; forged nonce and missing stale record refused; separate graceful stop verified exit (~1.4s including launch polling) |
| Per-app AX observers | Works on owned fixture | Two per-app native threads: created 1, window-moved 4, window-resized 4, moved 2, resized 2, focused 2, destroyed 1; engine receives all 16 events |
| Frame transaction | Works | Applied frame read back; exact preimage restored, 22ms including restoration |
| Partial write restoration | Works | Fixed-size helper accepts position, refuses size (`kAXErrorFailure`); applied frame proves partial write; exact preimage restored, 28ms. Restore-write refusal retained separately from successful readback |
| Focus / raise | Works on owned fixture | Initial activation/restore proof passed; activation 102ms, raise 12ms with AX focused WID/PID readback; original frontmost app restored |
| Stalled-app timeout | Works, bounded refusal | 20s owned-helper stall: 2s messaging timeout produces `kAXErrorCannotComplete`, cross-command authentication refuses within 3.4s; same helper authenticates after recovery (~1.5s including polling) |
| Cleanup | Works | Exact children gracefully stopped, reaped, records removed, process-gone true and CG residue false; no taps, hotkeys or Space writes |

The owned helper needed `finishLaunching()` to expose AX windows and AppKit
event draining to complete activation. Observer attachment before deferred
window creation proves the created leg. SIGTERM teardown produced no destroyed
notification; closing the owned window while its app remained alive delivered
one. The self-close marker reported invisible, and CG retained only an offscreen
record until app exit: on-screen absence is not total record disappearance.
An initial close-wait scheduling miss and a total-CG-absence assumption were
resolved by the helper visibility marker plus explicit lingering-entry readback.
No ordinary-app or physical shortcut acceptance is implied.

Independent review accepted the identity/unsafe/recovery/launch boundary and
verified fixes for destroyed-delivery evidence and misleading restore-write
wording. Final checks: 107 unit + 9 integration macOS tests, strict all-targets
clippy, formatting, signed bundle verification and shell syntax pass. Source:
`e9f628a` plus this milestone's probe/launch changes. Signed executable SHA-256:

- `tiler-macos`: `1cfb7a5aed2786179b19a6900273f18d660eb5fb75150dd256b3626600208d26`
- `tiler-macos-probe-target`: `4a7c5e44cb07d1b5f5630c452c3f51782d6b7e38bbe51d499106178b04f97d7c`

Phase 1 implication: retain per-app AX threads, maintained public bindings,
private identity join, bounded calls and readback-driven restoration. Hotkey and
native Spaces architecture verdicts await their live legs; native Spaces-first
is not yet established or rejected with SIP enabled.

## Earlier input registration milestone (2026-10-14; focus blocker superseded below)

| Capability | Verdict | Evidence and latency |
| --- | --- | --- |
| Carbon Ctrl+h registration / release | Works | Dispatcher-target route returns status 0; bounded 8s run and final retained-build 2s run end with no process. Zero deliveries without accepted test input is not delivery proof |
| CGEventTap create / release | Works for bounded idle runs | 8s observe-only and explicitly armed suppression runs create/release the tap; zero matched/suppressed events, no disable notification |
| Carbon delivery / exact suppression / inexact passthrough / post-release delivery | Blocked by fixture | Ctrl+h posts refused because workspace frontmost still identified the protected terminal; zero-event suppression and silence are vacuous |
| Timeout/user-input tap disable and recovery | Unverified | Zero disables observed; no actual disable/recovery evidence or independent hard-exit acceptance |
| Ctrl+arrow/digit collisions | Partial diagnostics only | Experimental Carbon left/right/up/down/digit1 registrations returned 0; Ctrl+Left tap observed two events and kept both (5s run). Single-Space layout stayed unchanged; shortcut enablement-to-chord mapping and native collision effects remain unproven |
| Native Spaces mutations | Untested | No temporary Space, move, switch or UI creation/destruction dispatched; no SIP rejection observed or inferred |

The legacy Carbon table failed closed with status -2 because
`NewEventHandlerUPP`, `InstallApplicationEventHandler` and
`DisposeEventHandlerUPP` were unavailable. The retained repair uses
`GetEventDispatcherTarget` + `InstallEventHandler` with the proc directly;
registration and handler are released before callback state. Pinned source:
[HotKey registration](https://github.com/soffes/HotKey/blob/a3cf605d7a96f6ff50e04fcb6dea6e2613cfcbe4/Sources/HotKey/HotKeysController.swift#L60-L67),
[handler](https://github.com/soffes/HotKey/blob/a3cf605d7a96f6ff50e04fcb6dea6e2613cfcbe4/Sources/HotKey/HotKeysController.swift#L158),
[unregister](https://github.com/soffes/HotKey/blob/a3cf605d7a96f6ff50e04fcb6dea6e2613cfcbe4/Sources/HotKey/HotKeysController.swift#L88).
Independent review accepted this minimal ABI/lifetime repair.

Two focus-fixture approaches failed: external activation/restoration using
NSRunningApplication and SetFrontProcess; then helper self-activation/yield
plus those external routes. AX focused PID and NSWorkspace frontmost disagreed,
and explicit restoration did not land within its deadline. The structured run
reported restoration failure even after exact helper termination; it did not
claim clean focus. That experimental input-session harness and all synthetic,
watchdog, helper-stimulus and CLI extensions were removed; only the reviewed
Carbon table repair remains. This is a fixture blocker, not evidence that
macOS denies hotkeys or that SIP rejects Spaces.

Fresh read-only checks after helper exit verified the original frontmost app
again and the initial Space layout/current exactly (`spaces:[1]`, `current:1`).
Runtime records are empty, all probes/helpers exited, no hooks remain, no
temporary Space was created, and no unrelated window was mutated. The retained
tree passes 107 unit + 9 integration tests, strict clippy and formatting;
signed rebuild/verification and LaunchServices trust true pass again.

Final retained signed build, source `fbaddfb` plus the Carbon repair:

- `tiler-macos` SHA-256: `e2eae926679efcd8d52c3106ee3079e7089d136d8845d6a8aa9f72ef1f3608aa`
- `tiler-macos-probe-target` SHA-256: `33ae89813437ead4b4402f8072cf347d40ed576fd556f7ed40957502e2f87563`

Phase 0 remains **incomplete**: AX implies a viable bounded per-app Phase 1
boundary; input effects and native Spaces-first viability still require live
acceptance. Keep tier-2 policy unchanged. A scripting addition with reduced SIP
would contradict that policy; parking remains an unselected alternative to
surface only if safe native routes fail. Neither alternative is justified by
the current fixture failure.

## SLPS focus investigation and current input status (2026-10-14)

The reusable focus implementation is **pending live acceptance**, not a selected
Phase 1 primitive yet. The probe-crate implementation includes the reusable
dispatch module, bounded `focus-session` command and opt-in regular-policy
helper; passing code gates and review do not establish live acceptance.
It resolves Carbon `GetProcessForPID` and SkyLight
`_SLPSSetFrontProcessWithOptions` / `SLPSPostEventRecordTo` behind the existing
safe FFI boundary. The focus dispatch uses mode `0x200`, followed by the two
`0xf8`-byte key-window records from
[yabai's focus functions](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1333)
and [ABI declarations](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h).
Unlike yabai's combined raise/focus function, this dispatch omits `AXRaise`.
Yabai obtains PSNs through other SkyLight functions; `GetProcessForPID` is the
probe's Carbon SDK lookup. No scripting addition or SIP change was used.

| Capability | Current verdict | Evidence and latency |
| --- | --- | --- |
| SLPS focus on Accessory helper | Partial effect; **required agreement failed** | Dispatch status 0 and both event-record posts status 0; AX focused PID/WID joined the owned CG window, but NSWorkspace still reported the original app. Dispatch 11ms, AX/CG polling 6ms. This is not a valid input fixture |
| Same-route original-app restoration | One earlier restore observed; latest fixture unproven | Ghostty NSWorkspace PID, AX PID/WID and CG join agreed after SLPS restore; dispatch 4ms, agreement polling 9ms. These are separate measurements, not an end-to-end latency distribution |
| Regular-policy owned helper | Implemented; full transfer unproven | Helper can appear frontmost at spawn and owns additional CG app-chrome windows. Fixture now joins CG owner set to AX window IDs, captures original focus before spawn, and restores original before the measured target dispatch if spawn took focus |
| Carbon delivery / exact tap suppression / passthrough / release delivery | Still unverified | No synthetic keys were posted in this investigation; earlier bounded registration/idle lifecycle evidence above is unchanged |
| Actual disabled-tap recovery / independent hard lifetime | Still unverified | No tap installed in this investigation |
| Enabled Mission Control binding mapping and collisions | Still pending | No shortcut settings changed; earlier registration-only observations do not settle enabled-binding behavior |
| Native Spaces writes | Still untested | Readback remained one Space, current Space unchanged; no create, move, switch or destroy dispatched |

The first Accessory-helper run failed the full agreement oracle. A provisional
AX/CG-only target gate was rejected and removed; NSWorkspace agreement remains
mandatory. Regular-policy fixture development then exposed extra CG windows and
spawn-time focus. Early fixture failures stopped helpers without complete focus
restoration; subsequent reads could not establish the original AX preimage.
The pending fixture now attempts identity-checked same-route restoration on all
post-spawn failures, retains failed restoration evidence, and checks agreement
again **after** verified helper exit. No legacy activation/self-yield route was
retried.

The final diagnostic established a different baseline: **NSWorkspace and
system-wide AX both reported `com.apple.loginwindow` frontmost**, a lock-screen
context. Ghostty's per-app focused-window join returned status `-25201`; multiple
CG windows meant a replacement WID could not be selected unambiguously. Recovery
was refused before any dispatch, and live work stopped. This does not establish
a persistent AX failure in an unlocked session. Unlocking and re-establishing the
original focus baseline is the immediate prerequisite; native Spaces probes are
not yet the next executable leg.

The last diagnostic used the fixed persistent-signed bundle with
`ax_trusted:true`, Secure Input Inactive and the existing designated requirement
(identifier plus persistent certificate leaf hash); signing verification passed.
Source basis: `d1007c5` plus the pending focus changes. The final **live** artifact
is older than subsequent code-only review repairs; no live result is attributed
to those repairs. Its signed executable SHA-256 values were:

- `tiler-macos`: `19dacfcdce6846d90768c03407f61b52a983fce62aa736aed86238da849dae5f`
- `tiler-macos-probe-target`: `22b931adfea5ab0e5893f21458ad2f0967ff35b71518eb296ad5ebd488c7e686`

Raw scoped diagnostic evidence remains outside repository records.
No probe/helper process or session record remained, and no tap/hotkey
was installed. Space layout/current remained unchanged. Original Ghostty focus
is **not** claimed restored at the final boundary: the desktop was locked.

Independent code review checked the pinned SLPS ABI, event-record layout,
identity/restore paths and unsafe isolation. Repairs removed the diagnostic
recovery CLI, added detailed AX errors and finite messaging timeouts, post-stop
focus readback, exact ordered per-display Space snapshots and failure on any
owned AX-window join error. Dispatch refuses a loginwindow/unknown frontmost
context; restore attempts retain distinct dispatch, skip and agreement causes.
Latest code-only gates pass: **115 macOS unit + 9
integration tests**, portable/macOS allowlist strict all-target clippy, formatting
and diff checks. Unsafe blocks in `ffi.rs`: **49 → 54 (+5)**; no unsafe added
elsewhere and no custom unsafe Send/Sync. Full-agreement focus and Phase 0 input
acceptance remain incomplete. No durable focus-route decision is promoted from
partial evidence.

## Native Spaces offline preparation (2026-10-14)

The [native-Spaces route survey and bounded plan](../research/macos-port/native-spaces-routes.md)
classify pinned yabai branches and nine other tools, separate source facts,
upstream claims and our readbacks, and define one-temporary-Space move/back,
switch/back, removal and ordered snapshot restoration. The bridged-operation
SIP-on report in yabai #2788 contradicts a blanket impossibility verdict;
compat-ID still lacks exact metadata restoration, and upstream bypasses direct
SLS moves on macOS 15+. No Space/UI/input write was run in this preparation.

Read-only hotkey inspection now distinguishes preference overrides from
effective Carbon chord dictionaries (CG versus Carbon modifier domains, no
array-index-to-symbolic-ID mapping). Personal bindings remain outside project
records; unresolved Desktop-N mappings must not enable settings implicitly.
This does not establish physical collision, delivery or suppression behavior.

Offline code preparation adds read-only user-Space type checks, an unambiguous
same-display bound and a non-loginwindow/readable-frontmost gate (rechecked
before dispatch) to both existing move and independent restore paths.
Explicit `--allow-untested`, identity, topology and membership
gates remain. Status/help now name non-SA Mission Control and system-chord
candidates as unimplemented/recovery-unproven, not rejected by SIP or tier-2
policy. Compat/bridged/UI/input writers remain unavailable. Phase 0 is ongoing;
no workspace-mode or SIP-policy decision is promoted.

Independent review accepted the pinned private ABI and scoped plan; its
cross-display finding is closed by the code bound. Frontmost revalidation
accepts a bundle-less helper only at the freshly authenticated owned PID;
unidentified foreign frontmost state still refuses. Final offline gates:
119 macOS unit + 9 integration tests, strict all-targets clippy on the
portable/macOS allowlist, workspace formatting and diff checks pass. Non-macOS
stubs were reviewed; native Linux/Windows compilation is not claimed from
these checks. The active Phase 0 record remains open for live acceptance.

Exact native-Spaces continuation: establish a fresh unlocked, signed/trusted
baseline with an owned disposable fixture and inspect the actual Desktop-N
shortcut labels read-only; then follow the survey's physical temporary-Space
setup and independent restore proof before any gated API experiment. The
full-agreement input fixture remains a separate outstanding leg; protected
terminal windows are never an agent restore target.

CI follow-up identified a pre-existing non-macOS strict-clippy failure in the
eight-argument helper stub (introduced with the regular-policy fixture): the
macOS implementation had the scoped lint allowance but its stub did not.
The stub now mirrors it without changing the signature or behavior. Corrected
revision `7655bb6` passes CI macOS and Windows build/test/format/strict-clippy,
and Linux workspace tests/format/strict-clippy
([run](https://github.com/beefsack/OmniTiler/actions/runs/38564752460)).
A separate pre-existing native-effect packaging failure omits the
macOS workspace member from its Nix source subset; it is not a Space-probe
runtime result.
