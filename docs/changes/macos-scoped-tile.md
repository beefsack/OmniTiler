# macOS scoped tile development mode

The [basic dogfood preview](macos-dogfood-preview.md) supersedes this milestone's
product input defaults, catalogue, disable policy and scope/lifetime prerequisites.
Its live evidence and original limitations below remain historical evidence.

## Goal and status

Deliver a bounded `tile --user-start --scope BUNDLE` preview over each display's
current ordinary user Space, using the shared Engine and real per-app AX
observation/actuation. Ctrl is configurable Meta; directional focus/move and
float toggle are the minimal input catalogue. Stop/crash leave frames in place.

**The scoped tap path is live-proven for basic tiling.** Full input acceptance
is still incomplete: physical Carbon delivery and an actual OS timeout-disable
notification/recovery have not been proven. Native workspace management and
effects remain later work. Ghostty remains excluded from development scopes;
its ordinary product classification is unchanged. No shared-core, Windows or
KDE source changes were required.

## Implementation and acceptance contract

- Main AppKit, dedicated input, per-app AX owners and one Engine owner retain
  the selected bounded-channel threading model. AX objects stay on their
  owners; no AX runs in the input owner, tap callback or production main.
- Complete title-free AX/CG facts feed shared classification. Unknown or
  incomplete snapshots preserve retained state; offscreen alone is not close.
  Native minima are queried when available; no maximum/fixed bounds are invented.
- Visible work areas convert AppKit coordinates to CG/AX points without
  backing-scale multiplication. Exact current-user membership and topology
  revisions fence each effect; failed topology reads invalidate authorization
  and identical-snapshot recovery republishes it. Multi-display/mixed-scale
  routing and input-current sets have fake coverage; live evidence is single-display.
- Latest-wins size → position → size transactions have per-leg identity,
  scope, membership, topology, stop/supersession and show-state fences, with
  partial-write readbacks. No parking, hiding, restyling or Space-management writes.
- The [tentative selected focus oracle](../decisions.md#scope-and-platform-goals)
  is SLPS-front PID == NSWorkspace-frontmost PID, plus that app's direct
  AXFocusedWindow (AXMainWindow fallback), `_AXUIElementGetWindow` and CG owner
  join. Foreground is checked around acquisition. Systemwide AXFocusedApplication
  and AXFrontmost are diagnostic-only. These are sequential narrow checks, not
  an atomic transaction. Every synthetic event edge uses the shared guarded
  boundary with owned-session proof and fresh exact-target agreement; Ghostty
  and protected targets refuse structurally.
- The 17 exact chords are Ctrl+H/J/K/L or arrows for focus, Shift for move,
  and Ctrl+G for float. Effective modifier collisions refuse validation.
  Carbon is the default discrete source; `--input tap` is explicit. No native
  shortcut settings are changed.
- Input eligibility precedes interception: scope, selected foreground,
  current-user membership, topology revision and non-fullscreen/show-state
  facts must agree. AX owners publish per-PID facts from their existing reads;
  the input owner consults plain facts and cheap foreground/session reads.
  Callbacks use atomics and monotonic time only. Facts expire after about 2 s;
  input attest cadence is 250 ms with a 500 ms callback deadline. Unknown/stale
  denies. Carbon releases registrations and taps release/recreate guards while
  gated. Genuine user-disable remains a refusal rather than being overridden.
- Window notifications are primary, including main-window and minimize/restore
  changes. Windows discovered by the scoped reader are subscribed explicitly.
  A deduplicated 1 s per-app complete-reread backstop recovers missed events;
  failure retry uses the same cadence. A hung app affects only its own owner.
- Both fixed signed bundles are assembled/verified: the probe is LSUIElement,
  and the dedicated regular helper uses `io.github.omnitiler.macos.probe.helper`.
  Helper records consistently accept exactly the probe/helper identities with
  executable/nonce/current CG/AX proof. Bounded stdout queues and non-panicking
  drains keep helpers alive when output consumers disappear.
- Normal finite expiry invokes bounded teardown; the independent watchdog
  bounds startup + lifetime + 5 s teardown + 30 s grace. Hard expiry reports a
  dirty process exit, not a successful JSON result. Independent holders permit
  geometry and input-release readback after tiler loss.

Unsafe stays in `ffi.rs`, with no custom unsafe Send/Sync. Maintained objc2
bindings cover public APIs. New input-session, CLI and native-backend modules
were split by responsibility below the production-file soft cap; the existing
large FFI boundary and runtime supervisor remain explicit structural exceptions.

## Defects found and corrected

1. Interrupted inherited edits left duplicate traits, inconsistent handoffs and
   stale backend APIs. Repaired and retained the sound native/core seams.
2. Defaults used Command rather than Ctrl, and Carbon Shift hits lost their
   modifier arm. Corrected defaults, full-pair conversion and effective-chord
   validation without changing shared shortcut policy.
3. Stale topology survived failed reads, and identical recovery could remain
   unpublished. Added invalidation and poll reset.
4. Systemwide AXFocusedApplication returned CannotComplete despite healthy
   direct-app AX. It no longer gates production or acceptance under the selected
   oracle. The first selected-oracle helper variants still disagreed with
   NSWorkspace: the main polling loop slept without servicing AppKit. Bounded
   main-thread event/run-loop pumping fixed target and restore agreement.
5. Probe foreground activation perturbed SLPS preimages; LSUIElement metadata
   prevents that launch-time disturbance. Regular fixtures prefer the separately
   signed helper. No missing-finishLaunching claim is retained: it already existed.
6. Minimize/close did not reflow: the scoped path had never installed window-level
   observer watches. Added watch synchronization, minimize notifications and the
   reread backstop; subsequent live reflow passed.
7. Global input interception could swallow chords outside the explicit scope.
   Added foreground/fullscreen-aware eligibility before suppression/registration.
   Review corrections retained per-app AX isolation and removed a callback mutex.
8. Helper stdout loss caused SIGPIPE/early exits; drains now retain the reader
   and tolerate closed output, with bounded counted queue drops. Fixture timers
   clamp to 20 s; later harnesses account for that rather than reading dead windows.
9. A lifecycle-log fixture waited on dispatch/counter evidence, not the engine's
   accepted result. It now waits for `ax-result/applied`, retaining its assertion;
   30 targeted repetitions and the final full suite passed.

## Live results — 2026-10-15

All key-event evidence below is **synthetic native CG pipeline evidence**, not
physical keyboard or visual shell acceptance. Controllers were launched through
LaunchServices; fresh trust/session/Secure Input checks preceded live legs.
Bounded display-sleep-prevention children covered live runs and were reaped.

| Leg | Observed result / latency |
| --- | --- |
| Focus original → regular helper → original | Pass, non-vacuous selected-oracle agreement. Target 15 ms dispatch / 142 ms dispatch-through-agreement; restore 2 ms / 114 ms. Later input fixture 141/111 ms. |
| Carbon input owner | All 17 registrations returned status 0, including arrow conflicts. To-PID, Session and HID synthetic producers did not produce a Carbon hit; physical delivery remains pending, not a proven Carbon/OS defect. |
| Tap exact suppression | Pass: matched Ctrl+h down/up absent from helper, owner hit/counters agree; representative 25–42 ms through owner observation. |
| Other-chord passthrough | Extra Alt modifier and plain-key buckets each received +1/+1. Ctrl+Shift+h is a move chord, not an unmatched test. |
| Explicit disable / fresh re-arm | Pass: actual old tap disabled, helper +1/+1; new same-chord guard epoch 1 enabled and suppressed exact pair in 40 ms. Same-tap re-enable produced a user-disable refusal; that behavior is not masked. |
| OS timeout notification | Not proven. Bounded 1.2/2 s callback stalls did not yield a verified timeout-disabled notification/recovery. Explicit re-arm is separate evidence. |
| Hard lifetime | Independent watchdog-owned tap process disappeared within 5 s with dirty-exit diagnostic; independently held helper survived and received +1/+1 after exit. |
| Mission Control conflicts | No settings writes. Read-only symbolic-hotkey preferences 79/80 were enabled. Left/Right exact tap chords suppressed in 28/36 ms with Space readback unchanged. Preference entries 32/33 were absent; no inference about their defaults. Up/Down overview actions and physical shell behavior remain untested. |
| Admission | Two owned helpers, and a fresh TextEdit instance plus helper, reached distinct half-work-area AX=CG frames. TextEdit admission was observed at the +6 s sample, not measured as a 6 s convergence latency. |
| Directional focus/move | Pass through actual tap chords and Engine dispatch; R/L selected the intended other helper. Native input dispatch→applied 9–24 ms in the reflow run. Move had signed geometry deltas and suppressed helper delivery. |
| Float/unfloat | Pass: floated frame remained byte-identical while the sibling changed; toggling back rejoined layout. |
| Minimize/restore | Pass after observer correction: survivor half→full work area, restored member re-admitted. Whole CLI action/readback windows 2024/1813 ms; includes LaunchServices overhead. |
| Close while app lives | Pass after observer correction: helper visible=false, process alive, survivor expanded; observed within a 4 s sampling window. |
| Calculator | Correct live ID is `com.apple.calculator` (case-sensitive). Standard window, AXSize not settable, no usable bounds: classified Tile under the selected hint policy, position moved to its slot, size remained natively clamped. Quiet rereads were stable, no write storm, helper continued tiling. |
| Panels/sheets | Real NSPanel and attached-sheet CG surfaces stayed unchanged while their parent/main windows tiled. They were not returned as separately joined AXWindows targets; no separate identity was invented. Native-child non-actuation passes; separate managed sheet identity remains TBD. |
| Native fullscreen | Owned enter read back true in 1409 ms wall; independent exit read back false in 1419 ms plus settle. With tiler active, fullscreen frame stayed unchanged; native Space creation/removal restored original layout/current. |
| Graceful expiry / forced kill | Pass: per-window AX+CG geometry unchanged across normal expiry and exact-identity SIGKILL. Post-kill helper received the formerly intercepted chord. |
| Scoped input isolation | Out-of-scope owned helper got +1/+1; in-scope helper consumed the same pair with native forwarding. Fullscreen interval had no tap forwarding; after exit interception resumed. Fullscreen downstream key delivery itself was not proven. |

The fixed-size/settability-only Calculator case follows the existing hint-based
decision; it is not evidence that a size-clamped slot is the final desired UX.
Distinct sheet identity, physical multi-display, floor-OS, games, physical Carbon
delivery and timeout-notification recovery retain their pending classifications.

## Evidence, final gates and cleanliness

- Source: `1903f92817d18e46f24fed43fc3b73b20842b4a4` plus this working-tree change.
- Final locked macOS tests: **406 pass**, no failures/ignored tests, doc tests
  complete. Portable core/protocol/effect-FFI/macOS allowlist tests and strict
  all-target clippy (`-D warnings`), workspace fmt, shell syntax and diff checks
  pass. The first [publication CI](https://github.com/beefsack/OmniTiler/actions/runs/38720616433)
  passed macOS but exposed missing macOS-only guards and Carbon stub exports
  in Linux/Windows builds. Those portability gaps are corrected without
  changing native behavior; cross-platform results are attached to the
  corrective publication.
- Portable verification also exposed a no-op display flag, incomplete stub
  teardown parity and test-only import/borrow issues; pure flag semantics are
  now shared and the stubs retain the native guard contract. An observation
  isolation fixture queued input before its autonomous focused snapshot had
  reached the engine; it now waits for that exact accepted sequence. Its
  assertions are unchanged and 40 targeted repetitions passed.
- Final `just app 'OmniTiler Mac Dev'` and `just verify` pass for both bundles,
  including strict signature and macOS 15.0 load commands. Development
  Gatekeeper rejection remains a diagnostic; no security policy was changed.
- Artifact SHA-256: probe `fae74da5d94a75c64c41b2c7ffeebad558c56d7908e41d950faa04153c5884b5`;
  helper `7813472a08a967a3e4a737961330509f39f49bb3c5488280e73720751776f398`.
  Respective identifiers are `io.github.omnitiler.macos.probe` and
  `io.github.omnitiler.macos.probe.helper`; both designated requirements bind
  certificate leaf `040f96377aaf110a77905d453bb76681e7133319`.
- Representative evidence sets: focus pump-fix roundtrip; input-session tap;
  direct-rearm second run; watchdog independent-holder run B2; tile slice E
  (reflow/focus), F (fullscreen/transients), ordinary-app scope inspections;
  final scoped-input isolation legs A/B/C2. Raw response bodies and artifact
  identities remain in ephemeral evidence rather than repository operational logs.
- Final checks at **21:14:32–21:14:37+1100 / 10:14:32–10:14:37Z** observed unlocked,
  AX-trusted, Secure Input inactive, Ghostty frontmost; a subsequent read joined
  SLPS/workspace/direct AX/CG on original PID/window 14379/396. Systemwide AX
  still returned CannotComplete and did not gate that result. No sampled lock
  occurred during this continuation; samples do not prove continuous lock state.
- Original final Space layout/current is `[1]` / `1`. Owned native-fullscreen
  Spaces were exited and removed; no Ghostty geometry/visibility/Space changes
  or synthetic Ghostty events occurred. Disposable helper/TextEdit/Calculator
  windows/processes, taps, registrations and sleep-prevention children were cleaned.

Earlier, before this continuation, a timed-out AppleEvents diagnostic used
broad-name osascript termination; unrelated-script impact could not be ruled
out. That historical anomaly is not reclassified as exact-identity cleanup.
The live legs above used exact recorded processes and no AppleEvents automation.

## Remaining / next action

The tap preview is the demonstrated route for bounded scoped dogfooding:
`just tile --user-start --scope com.apple.TextEdit --input tap --seconds 120`.
Scope remains app-wide; all windows of a named app are candidates. Calculator's
correct bundle spelling is lowercase. Explicit tap selection remains required.

Before removing development scope/lifetime guards for unscoped dogfooding,
complete physical Ctrl-chord Carbon delivery and OS timeout-notification recovery
or make a separately recorded input-route decision. Keep the sheet/fixed-size
discriminators and physical multi-display/fullscreen/game checks visible; native
workspace management and effects follow basic tiling. This note remains active
because the full requested input matrix is not complete.
