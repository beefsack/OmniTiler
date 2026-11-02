# macOS v4 native experiments

## Goal and scope

- Sharpen the outstanding M1-M9 macOS gates with native evidence, separating
  observations from implementation and acceptance. The desk review already
  exists; this record supplements it, not repeats it.
- Investigate native zoom, native fullscreen, system Fill and an explicitly
  owned work-area overlay as maximize candidates. Present evidence and product
  choices without selecting behavior.
- No core/protocol implementation, macOS v4 port, or changes to normative
  behavior, contract shape or decisions.

## Baselines and acceptance

- Initial experiment baseline: `36692ff0f142793f95230dc12b560a6178f2b66f`.
- Approved target: `e57030579b79634c95594abc89c9311223837707`, including
  the earlier macOS review at `c94bae10`, M1-M9 resolutions at `b2420acc`,
  and phase-0c approval. Subsequent experiments use this source baseline;
  its Rust implementation is unchanged from the initial baseline.
- The earlier review is in `docs/changes/v4-migration.md` at the target pin;
  its native gates remain open. Do not report resolved draft issues as new.
- Acceptance: report M-numbered evidence and remaining discriminators; compare
  maximize routes against shared overlay semantics; distinguish direct native
  observations, inspected source, earlier reports and unknowns.
- Native acceptance requires scoped identity/readback/restoration evidence per
  [live guide](../live-macos-testing.md) and
  [fixture reference](../../crates/tiler-live-fixture/README.md).

## Outcome and limits

- **Partial investigation; not native acceptance.** M4-3 directly pressing
  `AXZoomButton` entered native fullscreen and created a native Space, consistent
  with the modern green-button behavior, not ordinary zoom. No second press was
  attempted. The button is not a maximize executor; the Window-menu Zoom route
  is a separate mechanism. Fixture close/shutdown restored Space rows and focus.
  The retained M4-3 baseline replacement was subsequently identified as a system
  text-input auxiliary process: benign host churn, not app-window drift. No
  contract, behavior or capability selected.
- The restart-identity experiment omitted the required raw protected-window
  baseline. Its fixture observations are retained, but collateral-window
  restoration cannot be verified. This limitation cannot be repaired
  retrospectively. M4-3 used a new complete before/after baseline, not a repair
  of that earlier evidence.
- M2/M3 sequential churn and same-login token observations, six Window-menu
  legs and read-only M9 metadata capture now supplement the earlier evidence.
  M7 has terminal-attributed local diagnostics only: its unsigned observer did
  not follow the signed-bundle route, so no signed-probe event acceptance is
  claimed. M8 remains procedure-only. No conformance, persistence/recovery,
  physical-input, shell, game or anti-cheat acceptance; existing OPEN gates stay
  OPEN.
- A guarded cross-process AX frame writer and precise numeric launch-date
  diagnostic are now offline-tested and independently reviewed. The planned
  AX-write legs did not start: the graphical session locked during setup before
  fixture launch. No new native arbitration or numeric-token observation is
  claimed; locked-session setup captures are not valid experiment baselines.
- Later post-login preparation also stopped before fixture launch: the diagnostic
  execution context could not see the persistent signing identity or current-login
  graphical resources. Direct CG/Space captures returned null. A fresh user-terminal
  identity query did find the single persistent self-signed identity; its untrusted
  status is expected, not a reason to change trust. No valid new baseline, AX-write
  result or numeric fixture sample exists; no trust or consent change was made.
- Subsequent current-login execution recovered signed preflight and two AX-write
  lifetimes ran (M4-10/11 below). Zoom ultimately restored F0, not the intervening
  AX frame F1. These are limited end-state observations, not accepted live legs:
  one protected-roster mismatch was unresolved before the next lifetime, latest
  probe-source freshness was not established, and timing/intermediate-frame
  capture is absent. The Fill sequence omitted the final Zoom action.
- Revised Test 1 (M4-12/13) completes both action sequences with a fresh signed
  timing build: A restores F0; final B Zoom leaves a near-fill frame, not F0.
  Bounded common-monotonic AX sampling captures intermediate Fill frames and
  stable tails, not display smoothness. **Not accepted:** display/separate-Spaces
  baseline categories remain incomplete and B has unresolved ControlCenter
  onscreen geometry drift. Earlier A drift/continuation deviation remains intact.

## M-numbered findings

| Gate | Classification / new evidence | Remaining experiment or proposal |
| --- | --- | --- |
| M1 self identity / raw facts | Fits with capability; no new self-exclusion experiment. M4-2 reads real AX attribute names, not classification verdicts. Existing approved unavailable-fact list is not independently revalidated by this run. | Exercise bundled/unbundled own and foreign identity cases through the future v4 producer. Attribute availability on the fixture does not establish semantic ownership/transience or a route for every app. |
| M2 exact window lifetime | Sequential 10x10 close/reopen windows have distinct CG IDs; sentinel retains ID across fresh signed AX observers. Second fixture lifetime has a new PID/date/window ID. Positive close corroborated by AX disappearance, while CG entries linger offscreen. | Finite non-reuse is not a uniqueness theorem or actual tiler adoption. Preserve retained AX continuity/positive-close fences; restart exact binding still needs implementation and acceptance. |
| M3 restart store | Prior same-login observer audit equality observed. User reports unchanged audit user and changed observer audit-session ID across one physical logout/login; Dock birth fields unavailable. Additive numeric-date diagnostic implemented; no store written/adopted. | Compare numeric readings on a signed fixture with a non-null launch date; production still binds the whole-second string. One re-login discriminator is not a uniqueness/reuse theorem or store acceptance. Validate the complete namespace before adoption. |
| M4 maximize | Cross-process Zoom -> AX F1 -> Zoom -> Zoom restores old F0, not F1. Fill -> AX F1 -> Fill -> final Zoom leaves near-fill, not F0. Bounded common-clock sampling now retained; no independent zoom fact. Left-half stays same-Space; green button is fullscreen. | No neutral state producer/born detection or Return selector. Complete-baseline native acceptance remains blocked by rerun display/separate-Spaces gaps and B ControlCenter drift; no route selected. |
| M5 causal Space creation | Unknown; no new creation experiment. Existing review's same-call list-delta limitation remains. | A serialized, competing-creator discriminator must establish causal identity or keep `created:null`; no list-delta promotion or retry. |
| M6 Space retirement | Fits mechanisms with unverified integration; no removal experiment. | Verify core `exist:false` plus immediately fresh complete occupancy/pin/minimum checks. Do not infer removal authority from local creation or shutdown. |
| M7 / K5 border witness | Limited unsigned terminal-attributed 68-window diagnostic joins callback elements to exact IDs. App move/resize/focus delivery and per-window minimize/restore differ; cap drops per-window coverage. No signed-probe acceptance. | Repeat through signed probe with one monotonic causal clock and capture/event/render watermarks. Suppress unwatched/stale eligibility as approved. No evidence requires a new FrameWitness field, and local feasibility does not close K5. |
| M8 cross-display arrival | Unknown; not testable in the captured single-output topology. Separate-Spaces on/off and destination mid-switch/fullscreen variants not run. | Use two outputs and compare target frame plus exact native Space membership/current rows, not AX position alone. No settings changes or cross-display proof attempted. |
| M9 output identity | Public read-only helper obtains non-sentinel vendor/model/serial and display UUID; two fresh reads agree and UUID joins topology. User reports same-display sleep/wake stability; retained before/after metadata files are identical on inspection. Unit zero is valid. | Reconnect/re-login/identical-monitor stability and connector normalization remain unproved. Keep connector null; nullable metadata alone does not solve ambiguous identity. |

- W3/W1: local yabai source reconfirms its foreign-window sticky/above writer
  routes through the Dock scripting addition requiring reduced SIP, not the
  selected SIP-on route. No new SIP-on mechanism was established.
  The approved false capabilities remain provisional route descriptions, not
  a universal API-impossibility claim. Own overlay levels/all-Spaces flags
  would not prove authority over another app's windows.
- Units: the fixture's physical content size converts to AX/CG logical outer
  geometry using observed backing scale and chrome difference. This supports
  the existing point-space distinction, not mixed-DPI/drag/gesture coverage.
  v4 report scale remains 1 for that point coordinate space.

## Native experiments (2026-11-02)

### User-reported physical checks (not agent-controlled legs)

- Finder started small. The user reports Option-clicking green reduced it to
  about half height; a second Option-click restored the original size. With the
  titlebar double-click setting already Zoom, double-click behaved the same and
  the next double-click restored the original size. Green-button hover offered
  side tiling; holding Option offered quarter tiles. Neither menu offered Return
  to previous size.
- In TextEdit, the user reports titlebar double-click Zoom instead filled the
  area excluding menu bar and Dock, without entering fullscreen; green hover
  offered the same tiling menus. This directly supports application-specific
  standard/best size, not universal fill. No agent protected-window baseline
  exists for these user-owned checks; no mechanical collateral comparison or
  agent application-acceptance result is implied.
- The user ran the retained public display helper before and after physical
  sleep/wake and reported SAME from the comparison. Read-only inspection of the
  two retained files confirms identical metadata: display count, main/built-in/
  online flags, unit, vendor/model/serial, UUID, dimensions and bounds. Raw values
  remain external. This is user-reported same-display sleep stability, not all
  lifecycle, reconnect, identical-monitor, mixed-DPI or connector acceptance.
- Across a user-performed logout/login, the user reports the two retained
  identity files show unchanged observer audit user and changed observer audit-
  session ID. The target in each was the current-user Dock, with the same bundle
  identifier but unavailable launch-date/numeric-birth fields. Only one before
  and one after record exist; no new repeated same-login test is claimed. This
  is a useful single re-login discriminator alongside prior same-login equality,
  not store adoption, permanent uniqueness or PID-reuse proof.
- An unfiltered Dock process-name query initially returned multiple PIDs because
  multiple users were logged in. Select the current user's Dock using
  `mc-inspect` or a user-filtered query and reject zero/multiple matches. This
  explains a command-selection failure, not PID reuse.

### M4-1: existing fixture and observer

- Signed LaunchServices fixture bundle
  `io.github.omnitiler.live-fixture`; read-only probe bundle
  `io.github.omnitiler.macos.probe`, persistent development signer. No product
  tiler, event tap, fullscreen transition or Space mutation.
- Decorated/resizable fixture: `windowed -> maximized -> windowed` socket
  observations track native owner-side getter. A separately spawned
  `mode:maximized` window reports maximized on first observation.
- Independent scoped AX/CG joins corroborate frames and `fullscreen:false`;
  native Space membership/current remain unchanged. This is a native zoom
  versus fullscreen distinction, not proof of a cross-process zoom-state bit.
- Complete raw protected-window baselines were retained outside the repository.
  Fixture/process/socket teardown and focus return were observed; persistent
  protected-window geometry did not change. An unrelated offscreen auxiliary
  window was replaced at identical geometry; this is not claimed as an exact
  roster match.

### M4-2: AX inventory and frame-only counterexample

- Development-only additive observer diagnostic; no actions or setters beyond
  the existing AX messaging-timeout mechanism. Stable signing verified;
  signed launch reports `ax_trusted:true`, session unlocked before/after.
- Artifact SHA-256: probe
  `aeee3a9097543f67d1be4a69fc7789196b5e48346821e2bfaf1e3cec264a57e2`;
  fixture
  `8f27229aa63d9a969424dd6a666c51b22be874b942c0a1d30c9441fe247bc9b6`.
  These identify the experiment build, before subsequent offline-only
  diagnostic tightening.
- One decorated/resizable caller window was joined by fixture sentinel,
  process lifetime, AX window ID and CG owner. No unrelated titles/content
  were collected. Relevant raw files retained outside the repository:
  `scope-windowed.json`, `scope-maximized.json`, `windowed.json`, `fill.json`,
  `scope-filled.json`, `restore.json`, `scope-restored.json`, protected-window
  before/after records and artifact/signature records.

| Sequence | Native observation |
| --- | --- |
| Spawn windowed | Owner-side mode windowed; AX/CG frames agree. |
| Request native maximize | Accepted; owner-side mode maximized; AX fullscreen false; no Space transition. |
| Request windowed, then fresh get | Owner-side mode windowed and exact initial frame restored. |
| Set only position/size to the previously observed zoom frame | No maximize request; retained request mode windowed, but fresh owner-side getter reports maximized. AX/CG frame and zoom-button facts match the native-zoom case. |
| Restore initial position/size | Owner-side mode windowed; exact initial socket geometry and AX/CG outer frame restored. |

```json
{
  "has_zoomed_attr": false,
  "zoomed": null,
  "zoomed_status": "kAXErrorAttributeUnsupported",
  "zoom_button": {
    "found": true,
    "via": "direct-attr",
    "enabled": true,
    "has_value_attr": false,
    "value_bool": null,
    "value_status": "kAXErrorAttributeUnsupported"
  }
}
```

- This subset is unchanged in windowed, zoomed, frame-filled and restored
  samples. It establishes absence/unsupported reads for these fixture
  elements only, not every application or all private AX attributes.
- Both zoom/fullscreen buttons are directly accessible and enabled, but their
  enabled state is not a zoom bit. No `AXPress` was attempted. Native owner
  zoom restoration is demonstrated; AX-button toggle reliability is not.
- Independent inspection confirmed frame-fill started from observed windowed
  state; it was not merely resizing an already-zoomed window.
- Teardown: caller list empty, shutdown reports zero windows, exact owned PID
  and socket gone, awake child reaped. Protected persistent windows did not
  move; one unrelated offscreen auxiliary ID changed at identical geometry.
  Native Space rows/current matched baseline. No restoration residue observed.

### M23-1: fresh observer acquisition (limited evidence)

- One signed fixture process created a sentinel plus five batches of twenty
  windows, all simultaneously alive. The first signed observer joined all
  101; subsequent observers joined the sentinel after the others closed.
- Raw observations: requested/observed/failed = `101/101/0`, then `1/1/0`
  twice; 101 distinct IDs in the first snapshot; sentinel ID unchanged across
  observer launches. `_AXUIElementGetWindow` joins succeeded; launch-date
  presence was true, but its value/equality was not recorded by this command.
- **Not an ID-reuse test:** no close/reopen cycle followed the destruction.
  Finite churn would not prove permanent session uniqueness in any case.
- **Incomplete baseline:** protected-window PID/ID/layer/onscreen/bounds were
  not captured. Fixture shutdown, PID/socket absence, unchanged Space rows and
  no explicit focus writes were reported, but collateral restoration cannot
  be verified. Creation can activate the fixture. This run is not marked
  accepted or clean merely because teardown succeeded.

### M4-3: direct button press (fullscreen finding, noted host churn)

- Native physical, single built-in display. Rebuilt and verified the signed
  probe, including the final tightened read-only inventory. LaunchServices
  preflight reported `ax_trusted:true`, unlocked session, Secure Input inactive.
  Probe SHA-256:
  `380803bf292845e14ed1b94d048ba0929fcb0b253480ced334666a684834d148`;
  fixture SHA-256 unchanged from M4-2. Signing requirements retained externally.
- Before fixture launch: unfiltered, title-free `cg-inspect` captured every
  protected window's PID, ID, layer, onscreen flag and bounds. Native topology,
  Space rows/current, focus and process baselines were also retained externally.
  The new pure-file `cg-diff` compares complete capture files mechanically.
- One ordinary decorated/resizable fixture window was freshly bound by signed
  bundle/executable/launch identity, authenticated socket caller/run tag, native
  sentinel, CG owner PID and `_AXUIElementGetWindow`. Initial AX/CG frames agreed;
  fullscreen was false and native Space membership matched current.
- Exactly one guarded `AXPress` on the directly accessed `AXZoomButton` returned
  `pressed` (about 159 ms call duration). The immediate after-inventory missed
  the window during transition; a subsequent independent signed inspection
  rejoined the same window and read `fullscreen:true`. AX/CG full-display frames
  and a new current native Space corroborated fullscreen. This is the modern
  green-button fullscreen route, not successful maximize evidence.
- No second press, synthesized input, fullscreen setter, or ordinary frame-write
  recovery was attempted. Existing owned fixture close/shutdown removed the
  window/process/socket and the temporary fullscreen Space. Space rows/current
  exactly matched baseline; original frontmost/focused window returned naturally,
  without a protected-window focus write. The bounded awake child was reaped.
- **Original mechanical comparison: non-clean**, exit 1: before/after 34 records,
  `changed:[]`, one missing and one added offscreen auxiliary window of the same
  protected process with identical layer/onscreen/bounds. No persistent-window
  geometry drift was found. Mission Control was not opened. Later read-only
  owner inspection identified the pair as the system text-input cursor service,
  not an app window. Recomparison with that explicitly verified background owner
  classifies **benign host churn: offscreen replacement, identical geometry**,
  exit 0 with raw roster retained and `clean:false` (not an exact roster match).
  This leg is clean-with-noted-churn. Onscreen, geometry or app-window changes
  remain drift. M4-1's analogous churn was reported earlier; its raw captures
  were not located for this owner recheck, so that attribution is not upgraded.
- Raw `cg-before.json`, `cg-after.json`, `cg-diff.stderr`, initial/after scoped
  inspections, button report, socket replies, Space/focus snapshots and artifact
  records retained externally. The observation and restoration above are
  supported by those records; no clean-leg or exact preframe-restore claim.

### M4-4 through M4-9: Window-menu Zoom, Fill and frame arbitration

- Added a minimal fixture-only standard Window menu: winit's inspected default
  supplies only the app menu. The fixture registers `NSApp.windowsMenu`, with
  a nil-target `performZoom:` item; no delegate or standard-frame override.
  Native menu actions use the signed/authenticated fixture identity plus fresh
  exact `AXFocusedWindow` and both frontmost-PID reads immediately before press.
- Six separate fixture lifetimes, each with complete protected-window,
  Space/current, focus and process baselines. All six comparisons: no persistent
  changed fields; only the independently verified system text-input auxiliary
  replacement, classified clean-with-noted-churn. Space/current and natural
  frontmost restoration matched. Fixture process/socket and bounded awake child
  were removed. These are standalone fixture findings, not core integration.
- Initial title/depth-only inventory missed nested tile items. Corrected bounded
  direct-plus-submenu matching uses Loop's fixed selectors, with title fallback
  only for fixture Zoom/legacy Fill. An identifier-only attempt refused before
  actuation because the handmade Zoom item has no identifier; combined matching
  then succeeded. Corrected windowed/Zoom/Fill/left-tiled inventories find enabled
  Zoom, Fill and left/right halves, but no Return selector. This is not a claim
  about menus in every app or OS release. Left-half press is observed; right-half
  actuation remains untested.

| Sequence | Native/socket observation | Restoration / limit |
| --- | --- | --- |
| F0; Window-menu Zoom; Zoom again | AXPress succeeds; owner getter windowed -> maximized -> windowed; AX/CG frame changes then exactly returns; fullscreen false and native Space unchanged | Exact F0 restored, independently repeated with corrected matcher |
| F0; Zoom; host socket geometry write F1; Zoom; Zoom | F1 holds; owner getter becomes windowed; next Zoom reaches standard frame, following Zoom restores F0, not F1 | Native saved preimage survived the ordinary host write; socket/AppKit path, not cross-process AX-write proof |
| Write F2 while windowed; Zoom; Zoom | Exact F2 returns | Saving the immediate user frame works when entered from this ordinary state |
| F0; Fill | Fill reaches same standard frame/owner getter as Zoom; AX fullscreen false; no Space change | Corrected Fill-state inventory still has no Return selector; no new readable AX zoom bit or button value |
| Fill; Zoom toggle | Leaves a near-fill frame rather than F0 | Zoom toggle is not a proven Fill restore; no exact preimage-restoration claim |
| Fill; socket geometry write F1; Fill; Zoom toggle | F1 holds and owner getter becomes windowed; re-Fill fills again; Zoom leaves near-fill, not F0 | Same path limitation; no Return executor found; AX-write arbitration pending |
| Fill; left-half menu item; explicit socket F0 restore | AX/CG left-half frame, owner mode windowed, fullscreen false, original Space unchanged; exact F0 after socket restore | Ordinary tiling, not fullscreen; explicit geometry restoration is not native Return proof |

- AX inventory remains unchanged across these states: `AXZoomed` unsupported;
  zoom/fullscreen buttons enabled but no readable boolean value. Equal Fill and
  Zoom frames/getters do not prove equal OS state or a neutral state bit.
- Final corrected-matcher artifact SHA-256: probe
  `49e1847dcb2d65becf45ba092a87493344e3d54447f8e241601bed6a93a2a5f7`;
  fixture `78ef107d5214b8cbc6b53518ba4f7aaff7729c403d7184ccab8f90752ca29fca`.
  Stable development signatures and LaunchServices trusted/unlocked/inactive
  Secure Input preflight were observed. Earlier four legs used an earlier probe
  artifact; their action/readback evidence is retained separately. Raw captures
  and socket summaries remain outside the repository. Mechanical baseline
  reports and representative action/socket records were inspected directly;
  lifecycle/teardown checks are experiment-runner observations.

### M4 AX-write follow-up: offline-ready, native leg not started

- Added `fixture-frame-write`, using the existing signed fixture/socket/caller/
  run-tag/process-lifetime/CG/AX authorization. It writes `AXPosition` then
  `AXSize` to the exact retained window, not through its control socket. Direct
  element targeting does not require the menu route's key-window focus gate.
- Independent safety review found two blockers, both corrected and re-reviewed:
  retain known per-leg statuses and before/after facts after a partial failure,
  and recheck session/cancellation between the two blocking setters. A partial
  failure reports `outcome:unknown`, exit 1, with no replay or automatic restore.
  AX-thread/hard lifetime bounds and timeout-unknown semantics remain explicit.
- Signed probe and fixture were rebuilt and verified. Initial signed preflight
  was trusted/unlocked with Secure Input inactive; the session subsequently
  locked during setup. No fixture window, frame write or menu press was started.
  Setup CG/Space captures taken while locked are not valid before-baselines;
  no completed leg or after-baseline comparison exists. The runner reported no
  remaining owned fixture/probe/awake process or socket.
- Prepared probe SHA-256:
  `590b9b87f1244939292715bc498ec2472819a289b4899175241f7c542c532814`;
  fixture `fcc05b533c0a91624ad9ea5746a9cb9dfbb6511b1cd6f0cf0830f128e2b5655a`.
  These precede the later numeric launch-date diagnostic change. They identify
  prepared artifacts, not an executed AX-write experiment.
- Remaining discriminator: fresh F0 -> menu Zoom -> one ordinary AX frame F1 ->
  Zoom -> Zoom; separately F0 -> Fill -> one AX F1 -> Fill -> observe Zoom.
  Compare native saved preimage against F0/F1. Zoom after Fill remains observation,
  never a substitute for absent native Return. Use fresh complete baselines.
- Post-login preparation was attempted with no reuse of old PIDs/resources or
  baselines. The runner reports the build could not find the persistent signing
  identity, while verification of both existing bundles returned
  `CSSMERR_TP_NOT_TRUSTED` and signature display showed unavailable authority.
  Direct read-only CG/Space capture errors were retained and inspected:
  `CGWindowListCopyWindowInfo returned null` and
  `SLSCopyManagedDisplaySpaces returned null`. These are failed pre-effect gates,
  not product attempts or evidence of a changed certificate/TCC grant. No signed
  bundle was launched, fixture window created or frame/menu effect attempted;
  no owned fixture/socket/awake cleanup was needed. Both sequences and the
  numeric fixture observation remain unexecuted.
- **Signing diagnostic correction:** the development-environment procedure accepts
  exactly one full-name match in `security find-identity -p codesigning`, including
  `CSSMERR_TP_NOT_TRUSTED`. Zero identities in the `-v` valid-only subset and that
  status alone do not establish a missing signer or changed trust policy. The user
  reports exactly one matching persistent identity from a fresh current-login
  terminal. Earlier valid-only evidence was overly restrictive.
- A bounded read-only follow-up retained the full build and diagnostic output.
  The diagnostic execution context instead reports zero **matching** identities;
  both latest bundle builders refuse at identity lookup before compilation/signing.
  Existing verification recipes return the trust error and remain unsatisfied;
  signature display alone is not a successful verification or consent check.
- System process enumeration sees the current user's live Dock and Finder, but
  the same diagnostic context has no Dock in its running-application table,
  unknown lock/console/login state and null native Space rows. Secure Input is
  inactive. This corroborates a current-login execution-context access mismatch,
  not absent graphical applications or a certificate-recreation requirement.
  The exact underlying context mechanism remains unverified. Earlier raw CG-null
  evidence remains separate; no new complete protected-window baseline exists.
- No signed probe/fixture launch or desktop effect followed these failures. Both
  AX sequences and numeric fixture equality remain pending until current-login
  context access, latest signed build/verify and all fresh signed baselines work.
- **Nonmutating layer after a user-reported OpenCode-only restart:** existing
  diagnostic binaries still report zero matching signing identities, unknown
  lock/console/login state, no current-user Dock in the running-application table,
  and `SLSCopyManagedDisplaySpaces returned null`. A fresh complete title-free CG
  attempt reports `CGWindowListCopyWindowInfo returned null`. System process
  enumeration still sees the current-user Dock; target bundle/birth reads are
  null through the same unavailable application table, not an independent birth
  result. Secure Input remains inactive. No build, signing, signed launch, fixture
  or desktop effect was attempted in this layer.
- The user subsequently reports that the same existing `spaces-diagnose` succeeds
  directly in the current-login terminal: bridge/class/function/selector available,
  nonzero connection and readable display/current Space rows. Membership is null
  because no window ID was requested, not a failed lookup. This establishes native
  Space access in that terminal versus failure in the diagnostic execution context;
  it does not establish the precise OS cause or restore signed baseline access.
  A bounded process-ancestry inspection corroborates a surviving pre-login execution
  context; exact process details remain external. No live fixture/build followed.

### M4-10/11: AX-write arbitration, limited end-state evidence

- Current-login signed LaunchServices preflight recovered: Accessibility trusted,
  unlocked/on-console/login complete, Secure Input inactive. Existing probe and
  fixture verification recipes succeeded with the persistent signer. No green
  AXZoomButton press, product tiler, event tap or fullscreen/Space mutation.
- Two distinct signed fixture lifetimes were joined through caller/run tag,
  exact AX/CG window and process identity. Frames below are logical outer points;
  AX and CG agree at every recorded step. Both remain non-fullscreen on their
  original native Space. `Z` is the observed standard frame `(0,30,2240,1139)`;
  F0 is `(140,76,320,272)` in each lifetime.

| Leg / step | Observed frame `(x,y,w,h)` | Owner getter / interpretation | Diagnostic call duration only |
| --- | --- | --- | --- |
| A: F0 | `(140,76,320,272)` | Windowed | Not captured |
| A: Window-menu Zoom | Z | Maximized geometric getter | AXPress 0.091 ms |
| A: one AX position/size pair F1 | `(200,200,800,500)` | Windowed; requested F1 holds at readback | Write interval 73.701 ms |
| A: Zoom | Z | Maximized geometric getter | AXPress 0.074 ms |
| A: Zoom | Exact F0 | Windowed; native preimage restores F0, not F1 | AXPress 0.072 ms |
| B: F0 | `(140,76,320,272)` | Windowed | Not captured |
| B: Fill | Z | Maximized geometric getter | AXPress 0.071 ms |
| B: one AX position/size pair F1 | `(420,260,640,420)` | Windowed; requested F1 holds at readback | Write interval 69.089 ms |
| B: Fill | Z | Maximized geometric getter | AXPress 0.079 ms |
| B: final read-only zoom inventory | Z | Zoom action NOT pressed; restore target after re-Fill remains unknown | Not applicable |

- Both frame commands report `written`, position/size `ok`, exact immediate
  frame-after and no synthetic input. Menu calls report `pressed` with exact
  focus agreement. Unsupported `AXZoomed` and unreadable button values persist;
  Return selector remains absent. Zoom is not a native Return substitute.
- **Timing gap:** the durations above measure diagnostic call intervals, not
  dispatch-to-first-change, settled-frame or display latency. They exclude other
  proof/readback/launch work. No common dispatch/first-change/settled timestamps,
  intermediate-frame trace, animation duration, bounce-back or app-reassertion
  trace was retained. Point samples cannot establish jank-free transitions.
- **Mechanical baseline A: drift, not clean.** Complete title-free CG rosters
  have no application-window change, but one WindowServer high-layer entry
  changes onscreen true to false and vertical position by one point. Its
  cursor-like dimensions are inference, not a proven exemption. The narrow
  benign rule cannot waive onscreen or geometry changes. A separate freshly
  identified system text-input offscreen replacement has identical geometry.
  Its explicit benign classification leaves the overall verdict `drift`,
  `clean:false`. No retained resample resolves the WindowServer mismatch.
- **Mechanical baseline B:** no changed fields, only the same verified
  text-input offscreen identical-geometry replacement; explicit recomparison
  reports benign host churn with raw `clean:false`. This does not retrospectively
  clear A. Starting B before resolving A's mismatch remains a recorded deviation.
- Protocol disposition for a new bounded fixture-only run: the system-owned
  high-layer WindowServer mismatch is not reclassified as benign and A remains
  `drift` / `clean:false`. Nothing establishes fixture causation. For any such
  new mismatch, retain the initial after-roster and one delayed post-teardown
  re-sample, compare both before giving a verdict; do not widen `cg-diff`'s benign
  classifier. This permits a fresh discriminator, not retrospective acceptance.
- Each lifetime retained Space/current, frontmost, scoped fixture absence and
  unfiltered CG before/after records. Space/current rows matched and frontmost
  returned naturally without a protected-window focus write. However, retained
  files do not contain full display geometry/separate-Spaces or an independent
  process-baseline capture: do not claim every required baseline category.
- Fixture shutdown and empty after-scope are directly retained. The runner
  reports exact process/socket/run-directory removal and awake-child reap.
  Later read-only inspection confirms both sockets/run directories absent and
  no process at the four recorded fixture/awake PIDs; this is absence only,
  not permission to signal any reused PID. Leg A needed owned-bundle activation
  after socket focus was accepted without taking focus; no terminal input used.
- **Artifact limitation:** reused probe SHA-256
  `590b9b87f1244939292715bc498ec2472819a289b4899175241f7c542c532814`,
  fixture `fcc05b533c0a91624ad9ea5746a9cb9dfbb6511b1cd6f0cf0830f128e2b5655a`.
  These match the earlier frame-writer build, but probe `ffi.rs`/`main.rs` are
  newer than that bundle (numeric-date diagnostics added later). Latest-source
  rebuild was not performed; freshness cannot be asserted from the runner's
  earlier timestamp comparison. No numeric fixture sample was collected.
- Raw action/readback/baseline records were independently audited and selected
  records inspected directly. No overall PASS/native acceptance: A's mismatch,
  the missing final B Zoom, baseline/artifact gaps and new timing requirements
  remain unresolved. No further live leg followed the audit.

### M4-12/13: revised Test 1, timing and remaining baseline gaps (2026-11-03)

- Fresh signed probe and fixture builds reported successful; retained verify
  logs show valid on-disk signatures/designated requirements for
  `io.github.omnitiler.macos.probe` and `io.github.omnitiler.live-fixture`, same
  persistent development signer. Main probe SHA-256
  `9a5a02ceee9138006d57e8e2184971b994f5441d8dfe61d5baf9cc9e6b7eb975`;
  fixture `a010fd83e8df43acb17922d51837b1ff238a3b1f74b6ec4fa3f3564d871ce02c`.
  Independent hash/stat inspection confirms these identities and that the probe
  postdates `ffi.rs`, `main.rs` and all timing modules. Build stdout was not
  retained; build commands are runner reports, verify output is retained.
- Two fresh fixture lifetimes, one decorated/resizable caller each; signed
  before-preflight in both reports AX trusted, unlocked/on-console/login complete,
  Secure Input inactive. Every action and sampler uses signed LaunchServices.
  No product tiler, green-button press, event tap, Space/fullscreen operation,
  ordinary-app write or security/settings change. Exact menu focus is fixture
  focus; protected frontmost returns naturally only at teardown, not throughout.
- Frames are logical outer points; independent scoped AX/CG and socket readbacks
  agree. F0 `(140,76,320,272)` and Z `(0,30,2240,1139)` in each lifetime. Scoped
  samples remain `fullscreen:false`, same native Space. `isZoomed`/owner mode
  remains a geometric getter, not a neutral native maximized fact.

| Sequence | Result |
| --- | --- |
| A: F0 -> Window-menu Zoom -> ONE AX position/size pair F1 `(200,200,800,500)` -> Zoom -> Zoom | F0 -> Z -> exact F1 -> Z -> exact F0, not F1. Native saved preimage survives the intervening AX write |
| B: fresh F0 -> Fill -> ONE AX pair F1 `(420,260,640,420)` -> Fill -> final Window-menu Zoom PRESS | F0 -> Z -> exact F1 -> Z -> `(0,30,2220,1119)`, owner getter windowed. Near-fill, not F0 or F1. Zoom after Fill is observation, never native Return |

#### Timing method and per-step readbacks

- N=1 per step, eight actions. One process-common `mach_absolute_time` origin,
  converted with the host timebase; no wall clock or exchanged Rust Instant.
  D below is actual menu dispatch, or position dispatch for the AX pair, in
  host-monotonic ns. First-change columns are the read-start/read-completion
  bracket of the first fully post-dispatch AX sample differing from preframe.
  No sample straddled dispatch in this run. Position/size reads are non-atomic;
  a bracket is not the time of compositor-visible change.
- Criterion: every sampled frame in a trailing run agrees within 0.5 points per
  component with the final frame for at least 200 ms of read-completion stamps.
  Stable start is retrospective; confirmation is the first later sample proving
  that span, not the first stable sample itself. All relative columns are ms
  after D; raw integer stamps/brackets retained externally.

| Step | D (ns) | Call return +ms | First changed read bracket +ms | Stable start / confirmed +ms | Samples |
| --- | --- | --- | --- | --- | --- |
| A Zoom 1 | 185448659671458 | 0.068 | 19.109 - 538.553 | 538.553 / 761.898 | 156 |
| A AX F1 | 185468332569500 | 36.877 | 25.515 - 60.035 | 60.035 / 283.097 | 173 |
| A Zoom 2 | 185486694693750 | 0.088 | 5.443 - 495.589 | 495.589 / 723.751 | 154 |
| A Zoom 3 | 185501612594083 | 0.076 | 6.251 - 364.551 | 364.551 / 576.798 | 157 |
| B Fill 1 | 185602796207833 | 0.083 | 21.647 - 32.959 | 568.001 / 768.761 | 152 |
| B AX F1 | 185618255804625 | 68.060 | 27.740 - 85.172 | 85.172 / 307.609 | 166 |
| B Fill 2 | 185634097961166 | 0.105 | 34.908 - 61.044 | 515.907 / 719.441 | 156 |
| B final Zoom | 185649600236458 | 0.075 | 17.347 - 179.534 | 179.534 / 391.702 | 161 |

- AX size dispatch is D+0.875 ms in A and D+0.941 ms in B; each invocation
  contains only its one position/size pair. This cadence does not separately
  resolve their visual effects. Before-dispatch frame samples and final stable
  tails exist in all eight runs; zero clock misses, all sample statuses ok and
  exact CG owner agrees. Final stable sampled spans are 2.34-2.92 seconds.
- Fill 1 intermediates: `(134,74,390,304)`, `(70,53,1898,985)`,
  `(2,31,2205,1123)` then Z. Fill 2: `(392,245,988,577)`,
  `(91,79,2116,1083)`, `(2,32,2240,1139)` then Z. Each intermediate appears
  once. Zoom/direct AX samples show only pre/post plateaus, **not proof of no
  animation**. No return to preframe or later reassertion was sampled within
  these bounded tails; behavior between reads or after teardown is unknown.
- Sampling limit: 5 seconds / <=256 records/action, 20 ms requested pacing plus
  actual read/fence costs. Observed mean start-to-start cadence 29.1-33.0 ms,
  max gap 546.6 ms. AX read brackets reach 519.4 ms (Zoom 1), 490.1 ms (Zoom 2),
  358.3 ms (Zoom 3), 162.2 ms (final Zoom); Fill reads reach 180.6/173.9 ms;
  direct AX reads 34.5/57.4 ms. Thus first observed completion, especially Zoom,
  includes blocking observation, not measured action latency or macOS floor.
  No compositor/pixel capture, smoothness guarantee, percentiles or matched
  third-party/Windows/KDE benchmark is claimed.

#### Baseline verdicts and deviations

| Category | A rerun | B rerun |
| --- | --- | --- |
| Raw protected CG | 47 records before/after; initial drift from unlisted background replacement only. Explicit verified-background recomparison reports benign offscreen identical-geometry churn, raw `clean:false` | Same background replacement PLUS four ControlCenter layer-25 onscreen windows change x by +8 points, one width 151 -> 143. Explicit recomparison remains `drift` / `clean:false` |
| WindowServer / delayed re-sample | All six owner entries unchanged, including prior mismatch target; re-sample condition NOT triggered | Same: six unchanged; ControlCenter is a different owner, outside WindowServer-only re-sample protocol. Re-sample not applicable, not omitted clearing evidence |
| Space/current / fixture membership | Before/after native rows/current match; per-step fixture same-Space | Same |
| Focus | Frontmost and exact focused-window baseline return naturally; fixture menu focus agrees during actions | Same |
| Independent process / fixture absence | Raw process lists before/after, empty scoped fixture before/after, owned shutdown; recorded fixture/awake PIDs later absent | Same |
| Display geometry / separate-Spaces | Display geometry only before, no after. `spans-displays` preference key absent, not a proved separate-Spaces mode | No independent display geometry capture; no proved separate-Spaces mode. Space rows do not replace these categories |
| Signed session/consent | Before preflight and per-step/scoped session gates retained; no standalone after-preflight, scoped after-session retained | Same |

- Existing narrow background class was independently verified from raw process
  executable records; pure-file `cg-diff --benign-pid` was directly recomputed.
  No classifier code changed. B ControlCenter drift remains unresolved; system
  ownership does not make onscreen/geometry changes benign. No further live leg
  followed. Neither rerun clears the OLD A WindowServer drift or OLD B continuation
  deviation in M4-10/11.
- **Baseline protocol deviation:** execution proceeded despite incomplete display
  and separate-Spaces categories, including the second lifetime. These cannot
  be backfilled retrospectively. The runner's complete-baseline claim is rejected;
  no overall PASS/native acceptance or complete collateral restoration claim.
- Shutdown records and scoped absence are retained. Runner reports exact owned
  process/socket/run-directory teardown and awake-child reap; later read-only PID
  absence corroborates no fixture/awake residue at the recorded PIDs. No reused
  PID was signalled by that check. Raw data was stored outside the preferred
  dedicated temporary subdirectory; it was preserved, not deleted or relocated.
- Runner reports an initial bare-hash signer input caused sampler/action
  pre-effect signature refusal, then corrected to the full designated requirement.
  Those refusal files were overwritten: transcript-only, not retained evidence.
  No failed-effect replay is established by the surviving records.
- Runner unnecessarily repeated the full macOS library gate: 1,452 pass, same
  known hidden-focus test fails. Focused diagnostic gates remain separately green;
  no all-green library claim. No source was changed by the live runner. Latest
  verify output, raw action/scoped samples and comparisons were independently
  audited; selected records, hashes/timestamps and CG recomparisons inspected
  directly. Numeric fixture birth equality remains a separate unclosed M3 gate.

### GUI-session lifecycle risk and post-port follow-up

Observed context loss affected an OpenCode background service surviving logout;
the direct current-login terminal works. There is no evidence the macOS tiler
suffers the same issue. Cause is unknown; do not generalise this observation to
sleep. Potential generic host-service risk: a tiler or recovery helper surviving
a GUI session without reacquiring its context. After the macOS v4 port, test the
real packaged launch route, not raw terminal attribution: same-login lock/unlock
and sleep/wake; full logout/login with production lifecycle/relaunch; recovery
helper and session fencing; AX/CG/native Space access with the unchanged persistent
signer. Expected OS termination/relaunch is valid; surviving logout is not required.
Use fixture-only windows and standard baselines, rejecting stale prior-session
IDs/intents/native writes and permanently stranded hidden windows or intercepted
input. This is a follow-up risk, not a demonstrated tiler defect or normative change.

### M23-2: sequential lifetimes and login-token candidate

- Ten sequential batches of ten fixture windows, closed between batches, plus
  one surviving sentinel. All 100 batch CG IDs distinct; every socket close
  returns per-window success. Fresh AX observer sees only the sentinel after
  closure; closed CG records remain offscreen at their old bounds. Positive
  close is not inferred merely from a missing AX row.
- Fresh signed observers join the sentinel to the same AX/CG ID. A later fresh
  fixture process, with the same caller label, has a different PID, launch-date
  string and window ID. Launch-date string equals across three observations of
  the original process. `ns_app_identity_for_pid` formats NSDate debug text at
  whole-second resolution: this is not a precise birth-token proof or PID-reuse
  test. Preserve the underlying numeric date before depending on this fence.
- `proc-identity --pid PID` now reports target bundle/date beside the observer's
  own `getaudit_addr(2)` audit-user/session pair. Parent, direct and fresh signed
  observers agree within this login. The syscall is own-process-only; fixture
  audit-session membership was not independently read. `audit_session_self` was
  not selected; SDK marks that SPI private. Public `getaudit_addr(2)` supplies
  `auditinfo_addr.ai_asid`; no logout/login or reuse boundary was crossed.
- SDK `NSRunningApplication.h` warns against PID-alone process comparison and
  documents missing launch dates without LaunchServices. Candidate store key
  needs UID, boot/login namespace, precise birth and validated native window
  identity; audit ID alone is not yet proven sufficient. No inert record,
  atomic store, restart adoption or dependent write tested.
- Offline follow-up now reads one `launchDate` and preserves its finite
  `timeIntervalSinceReferenceDate` f64 bits as nullable `launch_ref_bits`, with
  `launch_ref_secs` as a display view in `proc-identity`. This public NSDate
  value is seconds since 2001-01-01 UTC; it is not parsed from the debug string.
  Missing/non-finite values remain null. Tests cover exact fractional/negative
  round trips, non-finite refusal, reserved PIDs and serde. Independent review
  confirmed existing production callers still compare the original string;
  the new value is diagnostic only, not a selected birth/login fence.
- No native numeric sample or fresh-observer equality result was collected.
  `KERN_BOOTTIME` is boot-only, not a same-boot re-login namespace. Own audit
  IDs now have the separately user-reported single re-login discriminator above;
  validated reuse handling and store adoption remain unproved.
- Full before/after protected comparison: only the verified system offscreen
  replacement, no changed fields; clean-with-noted-churn. Space/current rows
  match; both fixture processes/sockets and awake child are gone. These baseline
  files were directly recomputed; lifetime/teardown observations are retained
  experiment evidence. Signed observer artifact:
  `d0b4f93a423071353fd62fc66fd7fc18cf571219db807c60434ec2bde89d0f0b`.

### M7-1: local event feasibility, attribution deviation

- An ephemeral unsigned native observer ran directly from the shell, inheriting
  terminal AX attribution instead of the prescribed signed LaunchServices probe
  route. This deviation is unresolved; these results are local diagnostics,
  **not signed-probe/native K5 acceptance**. No product observer was changed.
- With 68 fixture windows, callback elements joined through CFEqual and
  `_AXUIElementGetWindow` to exact retained window IDs. App-registered
  `AXWindowMoved`/`AXWindowResized` and focused/main-window-change callbacks
  carried changed-window elements in this sample. Window-registered
  `AXMoved`/`AXResized` and miniaturize/deminiaturize differed; app-only
  registration did not deliver the latter. Per-window focus registration did
  not deliver the driven focus change even when its target was watched.
- A 64-watch sample excluded four of the 68 windows and missed excluded-target
  per-window delivery. A larger diagnostic registration joined all 68; this
  does not change approved fail-closed suppression beyond product coverage.
  Zoom caused identified resize storms, not a zoom-state notification/bit.
- Reported first-signal gaps were approximately tens of milliseconds for
  move/resize/focus and hundreds for animation/state changes. These combine
  separate wall-clock driver/start samples with observer-relative monotonic
  timestamps: not one causal clock, not end-to-end latency or completeness.
- Local exact-ID invalidation appears feasible; no evidence here requires a
  new core FrameWitness field. It also does not prove local sufficiency:
  signed-route repetition, monotonic capture/event/render joins, stale-event
  retirement and actual suppression remain OPEN. No fixture-protocol proposal.
- Full protected comparison recomputed: only verified offscreen background
  replacement, clean-with-noted-churn; Space/current rows match and owned
  fixture/process/socket/awake cleanup was reported. Attribution remains an
  evidence gap despite collateral baseline restoration.
- Offline source inspection confirms why existing signed diagnostics cannot
  simply reproduce the event trace: `ax_session_callback` discards the changed
  element and reports counts, while `phase1_observer_callback` only wakes/counts.
  `Phase1Observer::sync_windows` truncates to 64. Neither existing output joins
  callback WIDs to capture/event/render watermarks. A credible diagnostic needs
  reviewed callback ownership/teardown and bounded exact-ID event records;
  adding that instrumentation was deferred rather than extending product policy.
- A follow-up can keep driver sends, frame captures and callback timestamps in
  one signed probe process using one monotonic origin, with socket-driven fixture
  actions. Alternatively use a verified common host monotonic clock; do not
  exchange Rust `Instant` values across processes or combine launch wall time
  with relative monotonic offsets. Diagnostic watch coverage and suppression
  checks do not require a new core field. Actual renderer integration remains
  a separate unproved gate, not a diagnostic paint-success claim.

### M9-1: read-only metadata shape

- Ephemeral public CoreGraphics/ColorSync helper made two identical same-login
  reads. Vendor/model/serial were present and non-sentinel; UUID matched native
  topology. Raw values remain external. This proves lookup shape only, not
  physical identity/connector stability or a production report producer.
- SDK `CGDirectDisplay.h` defines null display ID zero. `IOGraphicsTypes.h`
  defines unknown vendor and generic product sentinels; normalize them to null,
  as well as missing reads/UUIDs and absent serial zero. Unit zero is a valid
  first unit. None of unit/display ID/UUID/ordinal proves a physical connector;
  keep connector null until separately normalized. Sentinel paths were inspected,
  not exercised on disconnected/unknown hardware in this leg.
- Exact-clean mechanical protected comparison: no missing, added or changed
  entries. Space/current and frontmost match; bounded awake child reaped. No
  physical lifecycle, second display or mixed-DPI transition was performed.
- SDK AX position/size are points; NSScreen backing conversion is distinct.
  CGDisplay pixel dimensions matching bounds in one read does not establish
  backing scale 1 or physical mixed-DPI behavior.

## Maximize options and recommendation (not a decision)

### Ownership and current contract

- Authority: [world-contract](../porting/world-contract.md), Authority (lines 7-15),
  distinguishes the implemented v3 boundary from the approved v4 target. The
  following is a target-fit investigation, not a claim that macOS implements v4.
- G4 / Ownership (lines 35-55) and [Facts versus classification and Intent
  ownership](../decisions/contract.md) (33-39, 63-69): core owns intent,
  classification, layout and presentation; adapters capture native facts,
  normalize coordinates, execute authorized mechanics and store opaque records.
  Exhaustive no-roundtrip exceptions are a last-moment safety veto and same-turn
  execution of an already core-decided result, not adapter policy.
- WindowFact (373-426): `maximized` is a nullable **native** fact; macOS
  `maximize:false` / `maximized:null` is provisional, with native read/write
  investigation required and geometry heuristics expressly disallowed.
  StateTarget (729-732) independently targets that native property. A successful
  Zoom call with null pre/post is only an attempt disposition, not a true native
  state report or completion (Raw attempts, 810-842).
- [R14](../decisions/contract.md) (127-133) puts maximize commands/refusals in
  core and leaves macOS native-versus-emulation OPEN. [Window-state overlays](../decisions/window-state.md)
  (3-38, 54-59) retain slot/share/allocation, prohibit geometry writes while
  natively maximized, require flag-preserving sends and observed clear before B9
  admission. Fullscreen exitability rules must not be misquoted as an existing
  generic maximize-exitability field. Native AppKit saved user frame is distinct
  from core retained tile allocation or intentional-float placement.
- Owned maximize does **not inherently move policy out of core**. Core-owned
  reversible overlay intent plus adapter frame execution/opaque storage fits
  the layering philosophy. It does not fit today's approved native-overlay
  behavior without changes: explicit external/native arbitration, restore,
  sends/workarea changes, birth/clear and recovery semantics need approval and
  core implementation. Geometry can already transport a single-tile layout via
  `alloc/frame` or explicit float `frame_op` (698-724), but does not thereby
  encode maximize intent. Schema cost depends on the chosen approved model;
  existing field semantics cannot silently be expanded.
- An adapter-private toggle/restore map deciding ownership and reporting native
  `maximized:true` would be policy plus a fabricated fact. Declaring emulated
  provenance avoids that lie only if a shared model explicitly consumes it;
  provenance alone does not authorize local decisions. Native KDE/Windows flags
  are not evidence for an equivalent macOS bit or an owned-geometry fact.
- M3 / Opaque intent (770-808): restart-bound exact lifetime identity remains
  unproved; IntentRecord currently contains `v/float/stickyPrior`, **no geometry,
  maximize arm, layout or history**. R12 (contract decisions, 119-125) persists
  preimages for hide/chrome/above/sticky, not generic maximize geometry. An owned
  overlay requires an approved preimage and restart/recovery design, not an
  unauthorized adapter semantic store. Composed sticky with nativeSticky false
  (world-contract 623-635) is a useful explicit core-policy precedent, not
  automatic maximize authorization.
- [Principles](../principles.md) Priorities (41-57) weigh magnitude, not rigid
  authenticity-over-latency ordering; Gaming absolutes remain absolute.
  Consistency (114-126) permits documented unavoidable host limits, not silent
  platform product differences. Layering (130-148) explicitly yields to simpler
  host-native mechanisms; Ordering (150-168) demands freshness, not extra delay;
  Efficiency (188-200) favors events and no pointless work. [VISION](../../VISION.md)
  Faithfulness vs Jank (21-26) leaves the subjective tradeoff to the owner.
  Emulated provenance need not automatically mean a per-platform setting: a
  neutral common policy/shared default or documented host limitation is possible.
- Approved M4 / phase-0c ([migration](v4-migration.md), 1151, 1207-1210): maximize
  stays in 0.1. False/null is not a scope cut; deferral requires a product decision.

| Route | Evidence / semantics | Contract and recovery cost | Responsiveness / risk |
| --- | --- | --- | --- |
| Native Window-menu Zoom | Fixture cross-process toggle restores F0; AX write F1 does not replace the native preimage. User Finder best-size shrink versus TextEdit fill; no neutral native state/born detection | Executor exists; native read, external actions, menu availability/key focus and core clear/send rules still unproved | Lowest executor addition; app-specific animation/reassertion unknown. Call duration is not visible latency |
| System Fill | Fixture work-area appearance and same-Space execution; Return absent. AX-write/re-Fill/final Zoom leaves near-fill, not F0; Zoom is not Return | Native state and reliable restore unavailable; cannot infer ownership or born Fill from frame | Three intermediate AX frames per Fill transition sampled; no pixel smoothness or relative superiority proof |
| Core-owned work-area overlay | Core reversible intent; adapter geometry, truthful native null. Can identify its own intent, not app-born zoom | Approved behavior/core changes; explicit base/restore/native arbitration/send/B9 plus M3 preimage recovery. Schema need depends on selected model | Leading shared-semantics candidate, **not measured faster or more reliable**. AX roundtrips and app response remain |
| Adapter-internal state with declared emulated provenance | Honest origin is better than a fabricated native bit, but private toggle/restore still decides policy | Requires explicit shared model/schema semantics and core adoption; not a local no-contract-change escape hatch | Local action may shorten one path but stale/dual ownership/restart complexity can add jank |
| Core layout-only work-area intent, no maximize | Existing geometry can represent work-area frame; truthfully a layout, not a native/owned maximize overlay | No new wire shape needed for geometry itself; reversible maximize/base retention still needs core behavior, or must remain a different action | Simple mechanics; gives up requested overlay semantics and native affordance integration |
| Native best-size plus calculated fallback | Native integration where available, fallback appearance elsewhere | No truthful maximize route from raw attempts alone. Must select shared fallback/ownership/restore semantics, not silently mix native and emulated facts | Different app paths/transitions, possible double action; no measured benefit |
| Provisional unsupported / 0.1 deferral | Current false/null is honest fail-closed evidence handling | Keeping it provisional is approved; removing maximize from 0.1 requires user decision | Avoids unproved effects, does not solve scope |
| Fullscreen substitute | Green button created fullscreen Space, not zoom | Reject: changes Space/visibility and collapses separate overlays | Different transition; not a maximize performance solution |

- Conditional recommendation: evaluate **core-owned work-area emulation**, not
  adapter-owned policy, as the leading shared-semantics candidate. Preserve
  false/null until an approved owned model or truthful native route exists.
  Native best-size is not universal fill; owned geometry is not a native flag.
  Do not trade an unmeasured latency advantage for unapproved semantics. The
  new near-fill final B result and sampled Fill progression strengthen the
  restore/transition caveats, not a native state producer or product selection.
  Zoom reads block longer in these samples; this does not prove a slower visible
  route or an unavoidable floor. Baseline gaps prevent native acceptance.
- User decisions: choose native best-size versus shared owned-overlay semantics;
  choose external resize/Zoom/Fill ownership arbitration and restart restore
  policy if owned; approve necessary core/contract changes or explicitly defer
  0.1. No decision/contract/spec/principles edit is made here.
- Sticky pager / Ghostty fullscreen-to-maximise on workspace return remains
  un-reproduced. No Ghostty manipulation or native-cause attribution occurred.

### Responsiveness: evidence, floor and design overhead

- First-class alongside ownership; no current measured macOS floor. User reports
  AeroSpace/yabai also feel janky and Windows/KDE feel snappier, not matched
  timing evidence. End-state equality, socket `settled`, AXPress duration and
  compositor-visible pixels are different claims. Test 1 lacks transition data.
- Parked latency trace patch was read, **not applied or compiled**. It joins
  diagnostic equality-only event IDs, dispatch windows and ingress receipts;
  no product fast path. Its `diag_now_ns` uses SystemTime/Unix wall clock, not
  a common monotonic clock, and cannot supply the requested timing protocol.
- Existing [managed-workspace evidence](macos-managed-workspaces.md), native
  select rows (312-325), records submit-relative existing-Space selects 43-61 ms,
  previous 48/56 ms, relative 50-62 ms; main-loop receipt was separately
  338-371 ms. Post-create dispatch/posted/confirmed/observed at
  915/917/945/1178 ms are different boundaries, not visual readiness. Sole-subject
  send (651) was 65 ms after native dispatch, 487 ms after engine receipt;
  bridged follow (656) 88-98 ms with first exact geometry 270/218 ms.
  These historical samples are not comparable p95s or fixed v4 costs.
- [Active-border discriminator](../spec/reference-outcomes.md#macos-active-border-discriminators)
  row (663): snapshot-to-application maximum 22-32 ms versus pre-fix 253 ms,
  explicitly not display latency. Physical smoothness remains TBD. Historical
  Mission Control/drag waits ([dogfood investigation](macos-managed-workspaces.md#dogfood-latency-investigation-2026-10-19),
  380-402) were implementation choices, not an inherent 300 ms macOS floor;
  later Dock-swipe routing superseded the default Mission Control path.
- Host mechanisms to isolate: per-window cross-process AX setters/readbacks,
  app event processing/animation/reassertion, notification delivery and
  WindowServer composition; existing-Space native switch versus creation or
  Mission Control traversal. Their existence does not establish unavoidable
  magnitude or that every app animates. App-wide animation-attribute changes
  require separate review, not automatic copying from reference managers.
- Added design costs to measure separately: authorization proofs, per-window
  redundant writes, capture/normalization, core execution, full-state report/reply
  construction/serialization/transport if used, queue/main-loop wake, observer
  coverage/delay and border render. Shared policy does not inherently require
  a macOS IPC roundtrip: core can be in-process. A 4 MiB protocol bound is not
  evidence that every action sends 4 MiB or that serialization dominates.
- [macOS decisions](../decisions/macos.md), Phase 1 skeleton (51-56), already
  selects per-app whole latest-wins batches/separate owners; native/idle cost
  remains unmeasured. Follow-up (94-99) shelves a broad trace programme but
  explicitly permits event-driven main completion wake without measure-first;
  trailing settlement (123-128) treats the old 50 ms pump as interim. Ingress
  (137-142) remains FIFO, no busy-drop. No new latency-policy decision here.
- Within v4: batch/latest-wins mechanics where permitted, same-turn execution
  of core-decided results, event-driven wake, remove redundant mechanical work,
  preserve causal freshness and anchored-border updates from later live frame
  events without per-frame core roundtrip (Presentation, 1061-1078). Suppress
  stale/unwatched eligibility beyond proven coverage. Not allowed: local target,
  layout, overlay eligibility, ownership/adoption or settlement policy, or
  optimistic predicted painting presented as authorized current state.

| Pinned local source | Narrow comparison, not a live benchmark |
| --- | --- |
| AeroSpace `74a1bf17e82d70e0a21945ba04bb4f590bf19f83`, `MacApp.swift:150-156,411-435` | Cancels stale frame jobs; per-app asynchronous size-position-size writes and `AXEnhancedUserInterface` false/body/restore workaround. Shows implementable suppression effort, not animation-free behavior or permission for ours |
| yabai `dd845723416f5fe92af49fad5ebab00369e07edd`, `src/window_manager.c:729-761,2384-2399` | Delayed AX events make cached-frame dedup unsafe; size-position-size workaround. Windowed-fullscreen retains frame and writes constrained bounds, not native maximize. SIP-on gesture capabilities differ from scripting-addition/reduced-SIP paths |
| yabai same pin, `src/display_manager.c:355-389` | Animation-state gate is explicitly unreliable on modern releases; not a trustworthy universal Space-settle oracle |
| Rectangle `1d46069` / Hammerspoon `23e387e`, source map below | Calculated usable-frame maximize and saved-frame restore are established precedent, not neutral native-maximized facts. Do not copy Hammerspoon's AX zoom-button route |

- Proposed bounded follow-up backlog item, not a programme: fixture-only small-N
  native Zoom/Fill/direct AX and, separately, existing-Space native versus tiler
  path; join dispatch, first frame change, stated settle criterion, intermediate
  frames/reassertion, notifications, core/reply/wake and overlay application on
  one verified monotonic origin. Report sample limits, not p50/p95 from tiny N.
  Compare against the achievable within-macOS mechanical baseline before matched
  Windows/KDE stages. No third-party live runs, parked-patch application, broad
  M7 framework, virtual-display research or second-monitor work is included.

## Inspected source and offline evidence

- Local winit `0.30.13`, macOS `window_delegate.rs`: `is_maximized` uses
  `NSWindow.isZoomed`; resizable `set_maximized` uses native `zoom:`;
  non-resizable behavior and style-mask changes differ. Fixture
  `win.rs::read_observed` intentionally avoids the getter for borderless or
  non-resizable windows. Do not broaden the decorated/resizable result.
- SDK `NSWindow.h` exposes in-process `isZoomed` and `performZoom:`;
  HIServices exposes `kAXZoomButtonAttribute` and `kAXPressAction`, not a
  documented `AXZoomed` constant. SDK search is not runtime impossibility.
- Apple [zoom](https://developer.apple.com/documentation/appkit/nswindow/zoom(_:))
  documents toggling application standard/user frames, matching within a few
  pixels, and a default frame that nearly fills the current screen. Its
  [standard-frame delegate](https://developer.apple.com/documentation/appkit/nswindowdelegate/windowwillusestandardframe(_:defaultframe:))
  selects the application's best information size (a document page may be
  narrower); the proposed default is screen-sized with interface reductions,
  explicitly including the menu bar. Exact equality with `visibleFrame` is not
  promised. Apple [isZoomed](https://developer.apple.com/documentation/appkit/nswindow/iszoomed)
  explicitly computes state by current-frame equality with the standard/default
  zoom frame. These primary docs corroborate M4-2, not an independent state bit.
- No custom standard-frame mode added to the fixture: it would need macOS
  delegate/winit interop, not an existing cheap socket option. Source establishes
  the best-size distinction without adding an unreviewed delegate replacement.
- Representative pinned local references: AeroSpace `74a1bf17`,
  `Sources/AppBundle/util/accessibility.swift` separates zoom/fullscreen
  accessors; button presence is used for classification, not a native state
  bit. yabai `dd845723`, `src/window_manager.c` zoom-parent/fullscreen are
  internal BSP-node zoom operations, not proof of native AX zoom. rift
  `8114ccc`, `src/sys/axuielement.rs` reads fullscreen separately. Rectangle
  `1d46069`, `Rectangle/WindowCalculation` implements calculated frame actions.
  These are source comparisons, not native acceptance or selected policy.

| Reference / inspected pin | Maximize-related source evidence (not native acceptance) |
| --- | --- |
| AeroSpace `74a1bf17` | `Sources/AppBundle/tree/MacApp.swift:411-421` writes calculated size-position-size. Zoom-button presence is classification, not state or actuation. |
| yabai `dd845723` | `src/window_manager.c:2326-2390` tree zoom changes BSP state; windowed-fullscreen retains `windowed_frame` and writes bounds. Native AX fullscreen is separate. |
| Amethyst `6508ee2` | `Amethyst/Layout/Layouts/FullscreenLayout.swift:18-32` uses `screen.adjustedFrame`; reflow writes calculated frames, not native fullscreen. AX implementation is in external Silica, not independently inspected here. |
| rift `ddc957a` | `src/sys/axuielement.rs:261,312-318` reads fullscreen separately from position/size setters; `src/sys/screen.rs:523` uses visible frame. No native zoom producer found in the searched source. |
| glide `2837add` | `src/actor/app.rs:359-364,1322-1323` frame setters; `src/actor/layout.rs:1025-1030` internal tree fullscreen, with floating restore frames. |
| paneru `b1b6abb` | `src/manager/windows.rs:359,406,428-430` AX frame writes; `src/ecs/display.rs:522` visible frame. Native fullscreen markers are not a maximize executor. |
| Rectangle `1d46069` | `Rectangle/WindowCalculation/MaximizeCalculation.swift` returns visible frame; `Rectangle/WindowManager.swift:75-96` restores saved `windowHistory.restoreRects`. |
| Loop `573ace3` | `Loop/Window Management/SystemWindowManager.swift:52-77,95` locates `_zoomFill:`/`_zoomUntile:` menu identifiers. `Loop/Window Management/Window Manipulation/WindowEngine.swift:139-163` requires frontmost app/enabled item before AXPress, with calculated-frame fallback. No truthful maximize bit established. |
| Hammerspoon `23e387e` | `Hammerspoon/HSuicore.m:784-797` native `toggleZoom` presses zoom button; `extensions/window/window.lua:589-591` maximize instead sets screen frame. Button presence is maximizable, not maximized. |

- Reference source comparisons above were inspected locally at the stated pins;
  Loop/Hammerspoon pins are the inspected clone revisions, not OS guarantees.
- M3 storage proposal: per-user `Application Support/OmniTiler` private
  subdirectory, 0700 directories/0600 files, UID/symlink checks, bounded
  versioned opaque records and atomic replace. Existing settings/recovery
  patterns are in `dev_selection.rs` and `desktop_shortcuts.rs`. Proposed
  key combines verified native window identity with process birth, bundle/
  executable and a validated login-session namespace, not title/PID alone.
- `CGSession` establishes lock/console/login state, not stable login identity.
  Own-process `getaudit_addr` provides a same-login equality candidate, not yet
  a tested re-login fence. Boot identity alone is insufficient for a same-boot
  logout/login. No store is written and no session fence is claimed sound yet.

## Diagnostic changes and verification

- Retain minimal reusable read-only diagnostic in `scope_inspect.rs` plus two
  FFI accessors in `ffi.rs`. It inventories bounded attribute names and reads
  named zoom/fullscreen-button metadata. It never presses buttons or changes
  production facts/classification, core or wire protocol.
- Independent review led to direct-accessor-only reads (no subtree search),
  explicit local versus native error status and attribute count/length/byte
  bounds. Runtime attribute names are app-supplied strings, not a content
  confidentiality boundary. The final tightened version was rebuilt/retested
  natively in M4-3; its unsupported zoom/value result remained unchanged.
- Final targeted tests: six `scope_inspect` tests pass; strict all-target
  macOS clippy and formatting pass. Earlier full library run: 1,397 pass,
  one existing failure
  `world_builder::tests::first_transition_same_report_focus_never_restores_hidden`;
  separately reproduced on the unmodified baseline. No all-green suite claim.
- Added pure-file `cg-diff`: missing/added/changed protected-window records
  retain `clean:false`; malformed rosters or duplicate `(pid,wid)` entries
  refuse. The
  envelope validates shape only; actual unfiltered `cg-inspect` capture supplies
  completeness. Clean JSON is stdout, mismatch JSON stderr. Capture before
  fixture launch and after complete teardown; re-sample after Mission Control
  closes if it was shown. Explicit `--benign-pid` is only for a separately
  verified system/unrelated background owner; only fully paired offscreen
  replacements at identical layer/geometry qualify. The verdict distinguishes
  clean-with-noted-churn (exit 0, raw roster retained) from drift (exit 1).
  Application-window churn, onscreen changes and geometry changes never qualify.
- Added development-only `fixture-zoom-press`, with authenticated fixture socket,
  signed bundle/executable/process-birth, CG owner, native caller/run tag and
  exact AX window/button joins. One press only, fresh commit gates, bounded
  reads, no synthetic input. Timeouts/errors are unknown effect, not retry
  permission. M4-3 proves the button name does not guarantee zoom: the command
  is not a production maximize executor.
- Added guarded `fixture-menu-press`, fixed five-item vocabulary, bounded
  selector matching and authenticated fixture socket/process/window identity.
  Independent review corrected main-window-only fallback, fabricated post-focus
  agreement, stale focus and late cancellation gaps. Exact focused-window and
  fresh session/cancel gates now precede the single press; timeout stays unknown
  effect/no replay. Fixture menu/AppKit wrapper and audit ABI were also reviewed.
- Added read-only `proc-identity` plus own-process audit FFI. No process paths,
  foreign titles, native store or v4 producer. Final small audit cleanup matches
  the SDK signed length type and removes a graphical-session assumption from
  its repeatability test; that cleanup supplies no new native audit observation.
- Added independently reviewed `fixture-frame-write` and the diagnostic-only
  numeric NSDate readback. The older prepared bundle executed M4-10/11 frame
  writes; it predates the numeric readback. Latest-source native proof remains
  missing.
- Latest directly executed checks:
  `mise exec -- cargo test --locked -p tiler-macos --lib -- fixture_frame fixture_menu fixture_zoom cg_diff scope_inspect cli_tests fixture_post audit_self launch_ref_bits ns_app_identity`:
  **146 pass**; `mise exec -- cargo test --locked -p tiler-live-fixture`:
  **122 pass**. Strict all-target clippy for both crates, workspace format and
  diff checks pass. The first fixture-test invocation exceeded the tool timeout
  after compilation without a test result; a subsequent bounded run completed.
  A prior full macOS run reported 1,431 pass / the same known
  hidden-focus failure; not a final all-green or native-acceptance claim.
- Revised Test 1 diagnostic: added development-only `fixture-frame-sample`,
  bounded to 5 seconds / 256 records, default 20 ms pacing plus actual read/proof
  costs. Concurrent menu/frame calls stamp actual action edges using the same
  `mach_absolute_time` plus timebase nanosecond origin. Each sampled AX frame
  carries read-start/read-completion brackets: position and size are separate
  reads, not atomic frames or display timestamps. Retained exact AX target,
  pre/post title-free CG owner fences, launch-token continuity and per-sample
  session/consent checks stop on uncertainty. No replay, writes or focus action
  in the sampler; existing effects remain one action per invocation.
- Independent review corrected pre-read timestamp bias, adjacent-pair drift
  falsely called settle, post-dispatch error evidence loss and late CG fence.
  Settle criterion is a final-anchor frame within 0.5 logical points/component
  for a trailing sampled span >=200 ms; short/clockless/failed tails are unknown.
  A further reviewed-path correction makes attempt classification independent of
  nullable clock stamps: a clock miss cannot turn an attempted error into a
  no-effect refusal. Observer overhead and visible smoothness remain unmeasured.
- After these diagnostic changes, runner logs show **155 focused macOS tests**,
  **122 fixture tests**, strict all-target clippy both and workspace format pass;
  selected logs and source were inspected directly. Independent re-review of the
  four initial blockers found no remaining blocker; clock-miss classification
  was subsequently regression-tested. No native sample accepted by offline gates.
- Next: resolve the reported baseline gaps/drift before any new native acceptance,
  signed-route event
  evidence repair, precise process/session fencing and user-owned physical
  procedures. No M-number gate is closed by the standalone diagnostics.

## Remaining bounded user-owned follow-up procedures

- **Native zoom/Fill physical interplay:** signed fixture only, capture F0 and
  complete protected/Space/focus baselines. Compare Window-menu Zoom with physical
  Option-green click and the current configured titlebar double-click action;
  do not change that preference. Ordinary green click is fullscreen, not a zoom
  retry. In a configuration exposing native Return, compare Fill -> Return and
  Fill -> ordinary frame write -> Return against F0. Current tested Return
  selector is absent: do not blindly repeat or substitute Zoom as restore.
  Read AX/CG/Space/socket state, close/shutdown, compare protected baselines;
  only separately verified background offscreen identical-geometry churn may be
  noted benign, never app/onscreen/geometry drift.
- **M3 re-login:** from repository root, run
  `mise exec -- cargo run --locked -p tiler-macos -- mc-inspect`; substitute its
  current-user Dock PID into
  `mise exec -- cargo run --locked -p tiler-macos -- proc-identity --pid DOCKPID`.
  Reject zero or multiple current-user matches; do not use an unfiltered Dock
  process-name lookup on a multi-user system.
  Retain two same-login outputs, physically save work/logout/login, repeat the
  two commands and compare observer audit pairs: audit user should match for the
  same user; a new login namespace must reject the old session. A changed token
  is only a discriminator, not store-adoption acceptance; no persisted record
  is currently consumed. The target PID supplies process metadata; audit fields
  always describe the observer, not Dock. Dock birth may remain unavailable;
  sample a signed fixture for numeric-birth equality instead. The one reported
  re-login discriminator above does not need repeating as a new same-login test.
  No shell variables required.
- **M8/M9 physical topology:** record complete title-free protected windows,
  display nullable metadata/UUID, topology and Space rows before a single
  user-performed replug, sleep/wake or re-login; take fresh observations after
  quiescence. Test identical monitors individually then together, preserving
  ambiguity rather than deriving connector identity from UUID/ordinal. With two
  displays, compare one owned fixture move's AX/CG frame AND exact destination
  Space/current membership; do not change separate-Spaces settings. Restore the
  fixture frame and original connection arrangement, then compare baselines.
  The retained public display helper supports before/after metadata reads;
  its exact temporary invocation is supplied with the external evidence, not a
  machine-specific repository path. Run identical-display tests individually
  then together; equal vendor/model or duplicated/absent serials must preserve
  ambiguity, not require invented uniqueness. Two-display acceptance remains
  pending; retain nullable connector and no settings changes.

## Proposed follow-up and decisions

- Possible future multi-display test path: virtual displays, for example the private CGVirtualDisplay route reportedly used by DeskPad/BetterDisplay; unverified, not researched, not current work.
- Proposed M4 backlog refinement only: retain maximize in 0.1 and OPEN product
  choice; menu Zoom executor/exact toggle restore proved, no neutral native
  state/born detection; Fill/half execution proved but Return absent. Limited AX
  end states preserve Zoom's F0 preimage; revised final B Zoom leaves near-fill,
  not F0. Common-monotonic bounded samples now exist, but display/separate-Spaces
  categories and B ControlCenter drift remain unresolved. Keep provisional
  `maximize:false` / `maximized:null`.
- Proposed M3 refinement: sequential non-reuse/fresh AX joins and same-login
  audit equality now observed; numeric birth readback implemented offline but
  unobserved natively. Require fresh-observer equality and validated re-login
  namespace before atomic store/adoption. M8 text needs no change.
- Product choice remains with the user. Recommendation is core-owned work-area
  emulation as the leading shared-semantics candidate, contingent on approved
  ownership/geometry rules and M3 recovery, with no measured latency advantage.
- Immediate outcome: revised action/timing discriminator delivered with evidence
  limitations, not accepted. No new live leg is implied. Missing before/after
  display and separate-Spaces categories cannot be repaired retrospectively;
  B ControlCenter drift remains unexplained. WindowServer-only delayed resample
  was not triggered in either rerun; no retrospective waiver of M4-10 drift.
  Signed-route event repair remains deferred;
  precise identity/session fencing is not adopted. Native physical, cross-app
  and M8/M9 lifecycle procedures stay
  user-owned. None of those gates is closed; no native acceptance while M7's
  attribution deviation or these gaps remain unresolved.

## Proposed behavioral discriminators (TBD, not normative edits)

| Given / action sequence | Distinguishing readback | Outcome |
| --- | --- | --- |
| Ordinary fixture F0; Window-menu Zoom; Zoom | Frame/getter, AX fullscreen, native Space, exact restore | Exact F0 restore observed, no fullscreen/Space change; neutral native-state/birth detection TBD. Green-button press is a distinct fullscreen action, not a zoom executor |
| Windowed fixture; OS Fill; ordinary frame write; re-Fill; Window-menu Zoom | System menu state, saved preimage and resulting frame | Cross-process AX F1 holds, re-Fill reaches Z, final Zoom leaves near-fill rather than F0. Native Return remains absent/TBD; no equivalent restore inferred |
| Fresh observer joins sentinel; repeatedly close/reopen other fixture windows; restart observer | Per-lifetime ID sequence, fresh AX joins, process birth/session fence; no stale-record adoption | 100 sequential lifetimes no observed reuse, sentinel fresh-joins, second process/new window IDs observed; exact restart-store adoption and re-login rejection TBD |
| Retain float intent; orderly owner restart within login; later new login | Same-live-window adoption versus old namespace rejection | TBD |
| If an owned overlay is selected: tile A; maximize; external resize F1; native Window-menu Zoom; clear owned overlay | Whether ownership yields, persists or reconciles; restored base F0 versus F1, without treating equal geometry as a native fact | TBD; proposed ownership discriminator only |
| If an owned overlay is selected: float A at F0; maximize; orderly tiler restart in same login; clear | Exact live-window re-adoption and restore preimage versus release of owned intent | TBD; M3 store has no approved geometry/maximize record |
| Start an app-created Zoom/Fill window; observe; invoke tiler maximize then clear | Native unknown versus recognized owned intent, B9 admission and exact restore | TBD; no birth classification from geometry |
| Owned overlay on A; send to another existing workarea; workarea changes; clear | Flag-preserving send, effective owned frame and retained tile/float base | TBD; core-owned model must define this before implementation |
| Watched fixture moves/resizes/focuses; repeat with >64 windows | Changed-window identity, capture/event/render ordering and suppression outside proven coverage | Unsigned local observer joins exact callback IDs and exposes cap/registration gaps; signed-route causal paint proof TBD |
