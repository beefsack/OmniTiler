# Windows gaming coexistence

## Shortcut-conflict proposal delivered (2026-10-16)

- [Research and proposal](../research/windows-port/xbox-shortcut-conflicts.md)
  records the approved Compatible rebind / Authentic configuration-conflict
  direction and Ctrl-first rule, current code bindings, alternate collision
  audit, Microsoft detection/navigation contracts, resolve-action risks and
  estimated implementation slices. No runtime behavior changed.
- Proposed float/fullscreen alternates are Ctrl+Win+G / Ctrl+Win+F11; F11
  physical nonactivation remains TBD. Game Bar current-user package presence
  has supported query APIs, but no supported Xbox enabled-state getter was
  established. Recommend official Settings navigation first, not direct
  registry writes or uninstall; detailed choices await review.
- Acceptance: source/catalog and Microsoft-reference review; conservative
  unknown-state semantics and minimal physical discriminators documented.
  Independent review checked navigation, package scope, registry boundaries and
  uninstall recovery; managed app-data loss and development-only preservation
  limits are explicit. Decisions/backlog link the proposal. Earlier
  no-replacement/no-selected-path options below are historical and superseded
  by the approved 2026-10-16 direction, not by implementation. The wider
  containment/runtime-detection
  work remains open, so this record stays active.

## Incident diagnosis and prevention options (2026-10-16)

### Scope and result

- Diagnose the reported Xbox-mode entry after fullscreen, invisible terminal
  on return, and temporary loss of tiling; reassess shortcut prevention without
  selecting new product behavior. Existing containment and documented-signal
  detection blockers remain open. No runtime correction is justified by this
  evidence alone.
- Evidence: supplied normal product log, 655 JSON lines, `trace:false`, plus
  current source and Microsoft references below. Line numbers below identify
  that supplied log, not repository files. `w19` is the fullscreen origin,
  identified as the terminal by the user's report, not by logged app content.
  No wall-clock timestamps delimit the incident; ticks establish order, not
  elapsed time. The user's physical report establishes Xbox entry/return;
  the log has no Xbox-mode identity signal.
- Acceptance for this investigation: distinguish observed actions from causes,
  identify missing discriminating evidence, compare prevention choices with
  existing shortcut and gaming decisions, and leave unproven outcomes TBD.

### Part A: reconstructed timeline

| Log lines / tick | Observed fact | Interpretation / limit |
| --- | --- | --- |
| 439-448 / 289-292 | `w19` maximizes, restores, then is eligible for reconcile | Ordinary operation before the incident |
| 449-455 / 295-296 | Consumed fullscreen down, `outcome:fullscreen`; border hidden for fullscreen; pager `suspended-fullscreen`; reconcile `windows:0`; taskbar still reported visible | Project fullscreen succeeded; pager suppression is intended. `windows:0` is zero eligible observations, not zero tracked windows |
| 456-481 / 298-354 | Repeated zero-eligible reconciles; fullscreen mask reports `mask-ok` | No geometry applied; mask insertion does not prove OS shortcut suppression |
| 482-504 / 355-371 | Foreground eligibility changes: no-target/fullscreen, pager elevated-foreground, one border `cloaked` reason at 368 | These are foreground/effect facts, not terminal visibility or Xbox identity. The cloaked foreground is not identified |
| 505-555 / 371-389 | Four `suspend cause:fullscreen-foreground` / `resume` pairs; overlays hidden; work-area change at line 530 | Generic fullscreen safety veto actively interrupts tiling. Not a documented Xbox-mode transition signal |
| 566-579 / 396-422 | One eligible `w78`, one placement applied; focus-mismatch rejection; zero eligible observations again; second work-area change at 577 | Intermittent visibility/eligibility and focus disturbance. No logged cause or old/new work-area rectangles |
| 580-595 / 425-428 | Select ws1 -> ws7 explicitly hides `w19` and `w78`; return ws7 -> ws1 reports `ok`, `no-focus`, aggregate reveal timing | Product workspace hide of the terminal is proven here, after the earlier zero-eligible interval. No per-window reveal outcome is logged |
| 596-606 / 434-445 | Zero eligible observations continue; later a min-hint change names `w19` | Cannot distinguish retained fullscreen, invisibility, cloak, or another classification exclusion from normal summaries |
| 607-618 / 454-465 | Consumed fullscreen down restores `w19`; eligible count becomes one, placements remain zero; pager briefly suppresses again then shows | Project-owned fullscreen persisted until this later toggle, after the workspace round trip. OS desktop return did not itself clear that marker |
| 623-637 / 473-474 | `w19` gestures refused as floating twice | Predicate is Engine exception OR floating workspace, not proof of an explicit float toggle or native window style. There is no float-toggle row explaining its origin |
| 646-655 | Clean input/render stop; four product-hide claims revealed, no residue | Recovery bookkeeping survived. This does not prove visibility throughout the return interval |

- High confidence: project fullscreen succeeded; the generic fullscreen guard
  suspended tiling four times; a later workspace select used product `SW_HIDE`
  on the terminal; project fullscreen only exited at tick 454; subsequent drag
  handling refused it through the floating/exception gate.
- Moderate confidence: the reported Xbox transition explains the unstable
  foreground and work-area signals. Microsoft documents one-window/fullscreen
  presentation and changed window-control behavior in Xbox mode. These symptoms
  are consistent, not a mode detector or proof of the exact OS mechanism.
- Unresolved: what first made the terminal invisible, whether its workspace
  reveal was followed by OS hiding/cloak or retained-fullscreen exclusion, and
  which exception/mode transition prevented later tiling. The explicit hide at
  tick 425 cannot explain the earlier zero-eligible interval by itself.
  No output-topology, session or desktop transition is established or excluded
  by this log. Work-area change is not proof of display reconfiguration.
- No logged evidence establishes a retry/backoff stall. Suspension demonstrably
  stalls active tiling during the four guard intervals; zero eligible windows
  and the later exception/mode refusal account for additional non-placement,
  but their underlying state transitions remain unknown.

### Source checks and observability gaps

- `tiling_sys/geometry.rs`: tick `windows` is `observed.len()`;
  `observe.rs` rejects invisible windows before classification;
  `observation.rs` retains tracked/hidden membership separately. A zero count
  cannot establish hidden, closed, floated, or lost windows.
- `tiling_sys/run_loop_tick.rs`: fullscreen veto skips active work and hides
  effects; resume clears the suspension and retry latch. `focus.rs` probes the
  foreground for border eligibility. Its `cloaked` reason does not name `w19`.
- `tiling_sys/selection.rs`: per-window hides are logged; successful and retired
  reveal branches are not. `product_hide.rs` uses owned, identity-fenced
  `SW_HIDE`/reveal claims, not an Xbox-specific DWM cloak operation. `ok` on
  return is aggregate success, not proof of subsequent compositor visibility.
- `tiling_sys/keyboard_overlays.rs` / `fullscreen.rs`: fullscreen enter/exit
  owns styles/frame state; exiting invokes reconcile. No mode-aware reset is
  implemented. `gesture_tick.rs` / `admission.rs`: floating refusal includes
  Engine exceptions, including born-fullscreen holds, or workspace mode.
  It does not distinguish their origins in the emitted row.
- Normal logs should reconstruct state transitions without requiring trace:
  bounded correlated reveal/readback outcomes per token; visibility/cloak and
  exception-lane changes with reasons; suspend/resume foreground safety facts
  using opaque identity; old/new output/work-area geometry; and enough
  focus/domain context to explain mismatch. Missing elapsed timestamps also
  prevent measuring the reported delay. These are follow-up diagnostics, not
  implemented changes or permission to collect titles/app content.

### Evidence needed before a correction

- Minimal discriminator recorded in
  [reference outcomes](../spec/reference-outcomes.md#windows-xbox-return-discriminator).
  Preserve an ordinary tiled origin and sibling, record baseline membership,
  then one physical fullscreen press; stop further gaming input on Xbox entry.
  Record entry and desktop-return times; use Task View's documented desktop
  return rather than another intercepted fullscreen chord. Observe the origin
  before any workspace select, then distinguish a workspace round trip and an
  explicit project-fullscreen exit as separate actions, if recovery is safe.
- Existing trace adds hook edge verdict/timing, observation/skip and placement
  detail; it does not supply all missing state transitions above. First design
  the bounded normal-log diagnostics or a read-only snapshot of visibility,
  cloak, owned hide claims, fullscreen marker, exception lane and workspace mode
  at each boundary. A trace-only repeat without those facts may remain ambiguous.
- No small decided-behavior fix is evident. Do not weaken fullscreen safety,
  invent Xbox heuristics, clear exceptions blindly, or rewrite hook threading
  based on this log. Native recovery and exact cause remain unaccepted.

### Part B: current Windows behavior and options

- Microsoft [KB5070297][xbox-mode], re-read 2026-10-16, documents Xbox mode on
  supported Windows 11 24H2+ installations/markets. Win+F11 enters/exits the
  full screen experience **when enabled**. This is distinct from Game Bar:
  Win+G opens Game Bar, whose Settings can in turn enter/exit Xbox mode.
  Desktop PCs expose Settings > Gaming > Xbox mode > Enable Xbox mode;
  handhelds use Choose home app, where None turns it off. Startup and Task View
  visibility options are not the same as disabling the mode.
- Current default classifier routes F11 to fullscreen and G to float (Shift+G
  sticky). With takeover and shortcut handling enabled, owned physical chords
  consume independently of foreground/action eligibility, and consumed holds
  stay consumed through release. `snapkey/sys/mod.rs` returns nonzero for a
  consume verdict and uses the E8 Win-release mask. No Xbox-aware exemption
  exists; action safety checks do not release Win+G to games automatically.
- Compatible disables conflicting physical bindings, including G/F11, with no
  replacement. Keep/Disable/Rebind are existing settings; Authentic remains the
  selected default. The recorded user decision explicitly treats an owned
  Win+G in a game as a tiler command, while disclosing incomplete containment.
- [LL-hook documentation][ll-hook] permits nonzero suppression of the remaining
  chain/target, but gives no shell-gaming ordering guarantee; timeout can silently
  remove the hook. Microsoft's [game shortcut sample][game-shortcut-hook] says
  swallowing Win does not block Game Bar hotkeys. That sample does not swallow
  G/F11, so it is not proof that all chord interception is impossible. Existing
  consumed-F11 plus visible-prompt evidence still rules out claiming success.

| Option | Effectiveness / official support | Reversal, gaming impact and posture | Fit with decisions |
| --- | --- | --- | --- |
| Keep/improve LL swallowing | Already implemented; does not reliably contain observed F11. No documented fix for both gaming shortcuts established. A dedicated thread is recommended for callback latency, not a proven gaming bypass fix | Setting is reversible; hook loss remains possible; anti-cheat acceptance unknown. No registry needed | Existing mechanism allowed; redesign requires separate evidence and choice |
| User turns Xbox mode off | Official feature setting targets F11's mode entry, not Win+G Game Bar. Physical suppression of prompts still needs verification | Reversible by user; removes Xbox mode availability but leaves Game Bar. No project registry writes | Previously proposed opt-in; not a selected product default |
| Game Bar Settings / Captures switches; `AppCaptureEnabled`, `GameDVR_Enabled` recipes | Recording/controller-open controls are not established Win+G keyboard-dispatch controls. No official current Win11 key contract or suppression guarantee established for these recipes | User-scope changes can still disrupt captures; restoring requires exact prior values, including absent keys. Do not silently write them | Not an approved project workaround |
| GameDVR `AllowGameDVR=0` policy | Official [ApplicationManagement policy][gamedvr-policy] disables Recording/Broadcasting, not a documented Win+G/F11 dispatch guarantee. Docs list Pro/Enterprise/Education/IoT and also say policy enforced only in Windows 10 desktop; do not promise Win11 enforcement | Device policy affects users/capture, requires management/admin authority; restore original configured/unconfigured state. No basis to apply it here | Official policy existence does not authorize writes or satisfy the shortcut outcome |
| Remove/disable Game Bar package | No demonstrated supported shortcut-suppression guarantee; missing package may leave activation prompts. Not a narrow chord control | Removes gaming UI/capture access; reinstall/update behavior needs verification; broad external mutation | Poor fit with preserving gamers' access; not recommended |
| Rebind fullscreen AND float, or choose Compatible/disable | Avoids accidental double action by not using G/F11 for tiler commands. Does NOT stop an intentional Win+G/F11 from invoking Windows | Fully reversible tiler settings; gamers retain OS chords; no policy/admin change. Compatible loses default conflicting tiler chords until rebound | Existing approved conflict model; no new default or replacement chosen |
| Tolerate and recover | Prevents neither shortcut. Automatic Xbox pause/recovery still blocked on documented signal; generic fullscreen guard is not enough | Preserves OS gaming features but accepts interruption and unresolved recovery defect; no registry change | Honest parked limitation, not acceptance of this incident |

- Recommendation for approval: use the existing per-binding rebind route for
  **both** fullscreen and float, leaving Windows gaming chords available. This
  avoids the incident during routine tiler actions without globally disabling
  gamers' features or claiming stronger LL suppression. It is conflict
  avoidance, not literal suppression of the original physical chords; replacement
  chords/default changes need the user's choice. Compatible is a broader existing
  opt-in if losing other conflicting bindings is acceptable.
- If retaining authentic F11 is essential, separately evaluate the official
  user-owned Xbox-mode-off setting. It addresses only F11, not G. No reviewed
  option guarantees suppression of both original chords while preserving them
  as tiler bindings and preserving Game Bar in games. That stronger requirement
  remains a containment research/release blocker, not solved by capture policies.
- Next product choice: keep authentic chords and accept/research the limitation,
  approve conflict avoidance/replacements, or opt into disabling an OS gaming
  feature. No new behavior, registry/policy ownership, default, or alternate
  Game Bar launch surface is selected by this investigation.
- Verification: documentation/source and official-page review only; no runtime
  changes. Existing runtime gates are not new evidence of Xbox recovery or
  shortcut containment. This active record remains open.

[gamedvr-policy]: https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-applicationmanagement#allowgamedvr

## PARKED: bounded install-order continuation (2026-10-03)

- Part (a) remains unaccepted. No official-API correction was established;
  stop further containment experiments pending the user's coexistence choice.
  Keep this record active. The earlier follow-up below is historical evidence;
  its proposed foreground-clear/install-order next step has now been exercised.
- Quick read-only prior-art check, at the revisions in the table below:
  - komorebi `docs/installation.md:17-24` warns about whkd Win-key restrictions;
    `docs/example-configurations.md:200-203` says whkd >=0.2.4 overrides most,
    with Win+L still requiring a registry change. Neither establishes G/F11
    containment. No separate local whkd clone was available; no clone attempted.
  - GlazeWM `keyboard_hook.rs:117-119,190-194` installs WH_KEYBOARD_LL and
    returns 1 for interception. `README.md:129` recommends Alt because Windows
    reserves some Win bindings. Whim `KeybindHook.cs:29-34,60,78-81` also uses
    WH_KEYBOARD_LL, consuming matched key-downs. No G/F11-specific guarantee
    or Game Bar-disable instruction was found in either inspected source.
  - Seelen `src/service/hotkeys.rs:12-20,44-70` uses win-hotkeys. Its matching
    cached dependency `9123179` (`windows-keyboard-hook/src/hook.rs:61,128-135,
    163-191`) uses WH_KEYBOARD_LL, return 1 and injected E8 masking. No G/F11
    containment-specific solution was found. These absences are search-limited.
- Microsoft [LL-hook docs][ll-hook] describe callbacks before posting to a
  thread input queue and permit nonzero return to block the remaining chain
  and target procedure. [Hook-chain docs][hook-order] put each new hook at the
  beginning. Neither establishes shell gaming-shortcut dispatch ordering.
  [RegisterHotKey][register-hotkey] reserves Win combinations for the OS and
  is not a demonstrated replacement. Microsoft's [game shortcut sample][game-shortcut-hook]
  explicitly says swallowing Win with WH_KEYBOARD_LL does not block Game Bar
  hotkeys (Win+G etc.). That sample does not consume G/F11; it is not proof
  that consuming those keys is universally impossible. No official fix for
  this host's consumed-F11 prompt was found.
- Live scope: physical Win11 build 26200, medium IL 8192, session 1, one
  2560x1440 display at 125%; existing gaming components preserved. Source
  `bbe5b0f` plus only the unaccepted fixture draft; fresh locked build owner
  SHA256 `553D839B9EC6B0503E941CF430308C9ECB753CA15CC1FDDD592298AB878679E3`,
  helper `8C9C2EBDFF54F9E525A690720692BE5A73AB02AEE85F1C1C9F52A03D2100E991`.
  Evidence root: `%TEMP%/opencode/gaming-installorder-20261003-203852-2136/`.
- Foreground cleared without gaming input: Explorer-brokered tagged helper,
  exact exposed-caption ancestor gate and one unmarked mouse click, cursor
  restored. Notepad activation preparation had instead merged into the
  original process; an Explorer error dialog was attributed to a Paint launch
  (launch not stored), then dismissed by one exact guarded OK click. No original
  app process was closed.
- One causal capture repair: DPI-unaware CopyFromScreen metrics cropped the
  2560x1440 desktop to 2048x1152, hiding the taskbar. PMv2 on the capture thread
  restored full screenshots. The black field was the expanded helper client,
  not a failed capture. No product/helper code changed. Preparation owners
  sent zero gaming input; timeout-expiry/restore and exact emergency-stop/restore
  were verified before proceeding.
- Comparison owner: `shortcut-proof --allowlist <round2/allowlist2.json>
  --seconds 300 --trace`, Explorer-brokered, exact ready identity, takeover,
  one eligible helper and `snap-available`. GameBar/FTServer/Widgets,
  EdgeGameAssist and XboxPcAppFT pre-existed this new owner. This establishes
  a later project-hook install, not the other components' actual hook order.
  Activation used raise/attach with E8 prime, not a click; the ancestor probe
  failed because of mixed DPI coordinates. The fresh exact-foreground key-send
  gate passed. No occlusion-free activation claim is accepted.
- `round2/shot-12-baseline3.png`: helper covering the work area with project
  border, taskbar/clock visible, no Xbox prompt. At 21:01:21 local, ONE marked
  Win+F11 tap inserted 4/4 transitions. Raw owner log
  `session-1/run-01dd532662fcc85d.log:311-331`: proof-marked F11 down/up
  consumed, 6/5 us; Win down present and Win-up passed; E8 filtered;
  mask send 470 us, inserted 2 / mask-ok, zero reported diagnostic drops.
  Project fullscreen action also occurred. These are callback/action facts,
  not shell-suppression proof.
- `round2/shot-13-f11-plus2s.png` and `shot-14-f11-plus10s.png` show
  "Switching to XBOX mode" in the exact Explorer-owned ApplicationFrameWindow.
  Prompt onset <=2 seconds; first-leak STOP, ZERO Win+G taps. Later install
  did not contain this F11 tap. Root cause remains unresolved: an earlier or
  outside-chain observer, queued callback delivery delay, or indirect
  fullscreen/mask/release activation. Fast callback timing does not measure
  message-pump wait. No hypothesis justifies a thread redesign here.
- Exact screenshot-identified foreground prompt received one unmarked Escape
  pair, no Enter or "Don't remind" interaction; prompt cloak read 2 afterwards and
  foreground returned to helper. `shot-15-after-esc.png` confirms dismissal;
  full Xbox mode was not selected. Exact graceful stop/restore and tagged
  helper close succeeded. `shot-16-end.png` confirms normal desktop, no prompt.
  `round2/end-state.json`: no project actors or ledger/request residue,
  SPI 0x0082=1 and 0x201E=35; original process starts and hosting Terminal
  survived. Both helpers exited, so no project overlay/hidden window remains.
- Independent review found two evidence projections to correct: the initial
  callback summary projected nested entries as empty vk0 rows; use raw lines
  above or `f11-correlation-corrected.json`. Activation JSON did not prove its
  ancestor guard; use `helper2-activation-correction.json`. Neither changes
  the correlated consume-plus-visible-prompt result.
- Cleanup exception: preparation launched Notepad into its existing process,
  but failed to capture a full pre-launch HWND set. Two extra empty Notepad
  windows may be launch-created residue. Their attribution is not established;
  they were preserved rather than risking closure of original user windows.
  Original Notepad HWND survives. See `notepad-attribution-correction.json`;
  user-owned visual identification/closure is the remaining desktop cleanup.
- Simplicity decision: discard the +880/-6 unaccepted GamingContainment draft
  from `scripts/windows-shortcuts.ps1`; its observer is not a suppression
  oracle, and expanding it does not solve the blocker. File restored exactly
  to committed baseline; existing mock passes (21 negative gates). No product
  behavior, binding, registry/policy or architecture change was made.
- Continuation verification: independent review accepted the corrected evidence
  and parking record. Locked four-package native build/test and strict all-target
  clippy, full rustfmt and diff checks pass. Delivery/hosted CI reported with
  publication; acceptance is documentation of the blocker, not gaming
  shortcut containment or complete desktop cleanup.

### Parked choices and recommendation

| Option | Boundary / consequence |
| --- | --- |
| Accept this OS limitation while retaining KDE bindings | Catalog stays authentic; F11 may also activate the Xbox prompt and G suppression remains unproven. This is acceptance of a known gap, not guaranteed impossibility. |
| User-applied Windows settings | Recommended first reversible route if preserving KDE bindings matters: Settings > Gaming > Xbox mode, turn off Enable Xbox mode (PC); handheld Choose home app > None is documented separately. Verify F11 physically afterwards. No documented current-host setting that reliably disables keyboard Win+G was established; controller-open/capture toggles are not that guarantee. Research/verify the exact Game Bar UI before claiming this solves G. |
| Registry/policy changes by project | Not authorized; affects host gaming behavior and needs a user decision plus bounded restoration semantics. Recording-disable policies are not established Win+G/F11 suppression guarantees. |
| Dedicated hook thread | Microsoft recommends it for prompt callback delivery, but no evidence here establishes pump wait as the cause. Architecture change needs authorization and a separate latency hypothesis/experiment; not selected. |

- Recommendation: keep part (a) PARKED and the KDE catalog unchanged; user
  chooses whether to accept the gap or personally disable Xbox mode and
  evaluate Game Bar's actual keyboard-shortcut controls. Do not spend another
  install-order attempt or grow the fixture. No provisional product exception
  was selected. Physical taps, holds/releases, plain Win/unowned chords,
  stop/reversal and ordinary-app/game feel remain user-owned.
- Proposed backlog text: "P0 Windows gaming coexistence (a) PARKED: later
  project-hook install still yields Xbox prompt after marked consumed F11
  (6/5 us, mask-ok, screenshots <=2s/10s); G withheld on first leak. No
  official-API fix established. User decision: accept limitation or personally
  disable Xbox mode/evaluate Game Bar keyboard controls; thread/policy changes
  need separate authorization. Record stays active."
- Exact next action: user identifies/closes any newly opened empty Notepad
  windows, then chooses the coexistence option. Resume containment only with
  that choice or new supported-mechanism evidence.

## Active follow-up: attributable live containment (2026-10-03)

- Scope now: backlog P0 part (a), authentic owned-shortcut containment on the
  normal desktop. KDE bindings remain mandatory. Placement defects D1/D2/D4/D5,
  Xbox detection and alternate Game Bar access are separate work.
- Physical dogfood T2/T3 still leaked Win+F11/Win+G after the earlier delivery;
  inclusion of `5746fca` in that payload is undetermined. T3 logged 36 consumed
  and zero passed classified events, without correlation to the leaking taps.
- Current source `4b3096a` includes both earlier fixes. Product hooks ignore all
  injected input; use the existing fixed-marker `shortcut-proof` path for
  automated classifier-equivalent evidence, never claim physical acceptance.
- Units: (1) source/artifact provenance and trace-only bounded callback decision
  and timing diagnostics; (2) normal-desktop owner-ready single F11 then G taps,
  observing OS surfaces, stopping/recovering on first leak; (3) evidence-directed
  supported fix or install-order comparison; (4) independent review, native/CI
  gates and handover. Extend conflict coverage only after the gaming taps pass.
- Live authority: user permits hooks, injection and owned
  windows on this physical host. Preserve the hosting Terminal process tree.
  End every run with no project process/overlay/ledger/hidden test window,
  SPI 0x0082=1 and 0x201E=35, and close only disposable apps opened for this work.
- Stop for undocumented mechanisms, registry/policy writes or architecture
  changes. Keep this record active until containment acceptance is established.
- Diagnostic/provenance unit accepted: `windows-dev.ps1` records source HEAD,
  dirty state, copied-payload SHA256, argv and exact ready identity in
  `<owner-log>.provenance.json` plus stdout. This is launch attribution, not an
  embedded commit claim. The owner log stays single-writer; sidecar-write failure
  requests verified stop/restore.
- Trace-only `snap-callback` reports bounded closed-catalog decisions, modifier
  state, source, local classification timing, mask-send count/max latency and
  saturating loss/privacy-filter counts. Collection is disabled without trace.
  No ordinary typed letters, app content or native window identifiers enter it.
  Armed state is captured before classification so early-Win-up chord releases
  remain diagnosable. Downstream-hook latency and time waiting for the owner to
  pump messages remain outside this metric.
- Independent review found diagnostic edge/privacy-overhead and concurrent-log
  writer/recovery gaps. One correction resolved them before acceptance.
  The final diff inspected; locked four-package native tests run, strict
  all-target clippy, full rustfmt and diff checks; the explicit locked build also
  passed. Snapkey: 58 tests. No shortcut behavior changed in this unit.
- `2c9c11c` hosted CI [37113756434][diagnostic-ci] passed all four jobs.
- Fixture contract accepted after correcting stop-after-recovery, consumed-action
  versus interception eligibility, project-overlay exclusion and newly-uncloaked
  observation. Post-tap sampling additionally bounded to two seconds and
  recovery fenced to the exact newly observed foreground shell/gaming surface.
  Mock: 44 negative gates pass. Test observer selectors are evidence-only;
  they are not a product Xbox-mode detector. Subsequent screenshot evidence
  invalidated their clean-desktop inference; the fixture remains an unaccepted
  working-tree diagnostic draft, not an acceptance gate.
- First live run (Win11 build 26200, medium IL 8192, one display):
  `target/windows-shortcuts/20261003-195626-33604/shortcuts-fail.json`, source
  `2c9c11c` plus the fixture diff; owner SHA256
  `553D839B9EC6B0503E941CF430308C9ECB753CA15CC1FDDD592298AB878679E3`.
  One marked F11 tap, zero G taps. F11 callback down/up consumed (12/15 us),
  mask inserted 2, mask-send 622 us, no diagnostic drops. Taskbar held;
  no selector-recognized Game Bar/Xbox surface appeared in those samples. At
  sample 2, new visible
  `EdgeGameAssist.exe`/`msedge.exe` windows made observation uncertain; STOP
  before G. They were preserved. This snapshot alone did not establish a leaked
  shortcut or prove whether fullscreen activity triggered Game Assist.
- First run stop/restore succeeded, helper closed, no project actors or ledger,
  SPI arranging=1, pen=35, hosting Terminal and original apps alive. Accessible
  exact-owner graceful and emergency routes were first proved with nonhook runs.
- Final outcome: containment goal NOT met. Read-only shell screenshot later
  confirmed the "Switching to XBOX mode" prompt in an Explorer-owned
  `ApplicationFrameWindow`, not a GameBar/Xbox-named process. It was absent from
  the first baseline window set. Exactly one gaming chord was sent across all
  runs (marked F11); G and the remaining collision matrix were not reached.
  Prompt onset was not timestamped, so direct key leakage, an earlier observer,
  delayed delivery and indirect fullscreen activation remain distinguishable
  hypotheses, not an established OS root cause. Current artifact inclusion of
  the earlier fixes and consumption of this tap are established.
- Ten-second no-input dwell was stable with GameBar and EdgeGameAssist running.
  Later-owner install-order comparisons were blocked BEFORE input by the shell
  frame's fullscreen foreground: first reconcile suspension, then foreground
  activation refusal. One causal fixture repair moved exact helper activation
  ahead of convergence; it did not solve refusal. Subsequent runs
  `20261003-200151-32208`, `20261003-200738-4544` and
  `20261003-202209-32864` contain zero F11/G taps. No install-order conclusion
  follows from owner/process start ordering alone; no further attempt is selected.
- Recovery: screenshot-verified exact Explorer prompt received one successful
  Escape down/up pair; it disappeared and cloaked 0->2 with NOACTIVATE. An
  initial malformed test INPUT struct inserted zero events and was replaced
  with the existing proven sender before the successful pair. No Enter or
  "Don't remind" interaction occurred. Full Xbox mode was never selected.
  The cloaked shell frame remained the stale foreground; raise/attach activation
  still refused. Post-recovery PMv2 screenshot confirmed normal desktop/taskbar
  and no prompt; cloak/foreground alone are not product mode detection.
- Final live end: all exact owners/helpers stopped and closed, restore true,
  no project actors/overlays/ledger/stop/workspace request residue, SPI 1/35,
  original Notepad/Paint and hosting Terminal alive. No ordinary apps were
  launched or closed; existing GameBar/Edge processes and windows were preserved.
- Delivery: accept the diagnostic/provenance unit only. Preserve the unaccepted
  fixture diff in `scripts/windows-shortcuts.ps1`; do not archive this active
  record or advance containment to done. No rebind, registry/policy write,
  undocumented mechanism or hook-thread redesign was implemented.
- Options/recommendation: first have the user click a normal app to clear the
  stale shell foreground, then resume a bounded install-order comparison with
  screenshot-confirmed baseline and delayed post-tap observation. This is the
  smallest next step; it needs no product architecture choice. If timing evidence
  then implicates installer-thread latency, propose a dedicated-thread experiment
  for approval. If supported interception proves insufficient, return a policy
  or mechanism tradeoff instead of silently disabling gaming surfaces.
- User-owned acceptance remains all physical colliding chords, plain Win and
  unowned chords, release ordering, holds and reversal/stop. Fast local callback
  timing excludes neither queued callback delivery delay nor silent later hook
  loss. Observed F11 consume/mask success is not suppression acceptance.

## Goal and scope

- Enforce the existing authentic Windows shortcut catalog without OS leakage,
  including unmanaged/shell foregrounds, repeats and release ordering.
- Find a documented Xbox full screen experience signal before implementing
  pause/restore of tiling, effects and shortcuts with preserved workspace state.
- Research alternate Game Bar access and anti-cheat implications of existing
  mechanisms, including Windows prior art.
- Exclude initial split-axis and unmaximise overlap defects.

## Acceptance

- Audit every owned chord and its Windows conflicts; extract relevant evidence
  from the user's 2026-10-03 trace without application content/raw identifiers.
- Any shortcut fix has meaningful offline regression coverage and passes locked
  native build/test, strict all-target clippy and rustfmt; pushed CI is green.
- Xbox detection uses a documented signal, not a cloak/foreground heuristic.
  If no suitable signal is established, document the blocker and options.
- Research distinguishes documented guarantees, source inspection, user
  observations and unresolved physical acceptance.
- Public Game Bar access choices, mechanism/security changes and unsupported
  shortcut interception return for a user decision.

## Bounded units

1. Investigate hook routing/catalog, trace evidence and official Xbox detection.
2. Implement and verify the supported authentic shortcut correction, if justified.
3. Research Game Bar alternatives, anti-cheat and local prior art.
4. Review risky shortcut changes independently; integrate records and evidence.

## Outcome and accepted implementation

- Authentic shortcut containment is implemented and offline-verified. Xbox
  automatic pause/restore remains blocked on a documented runtime mode signal;
  alternate Game Bar access remains a user decision. This record stays active.
- `snapkey.rs` separates interception from action eligibility. Existing owned
  fresh chords consume whenever takeover/shortcut handling is enabled, including
  unmanaged, unadmitted and desktop/shell foregrounds, normal fullscreen
  suspension, gestures and elevated foregrounds. The latter are not authorization
  to manipulate protected windows: owner identity/scope/integrity checks remain.
- Consumed holds retain their verdict and down-time operation through repeats,
  modifier changes and either release ordering. Repeats with a changed modifier
  combination, released Win or disabled handling do not dispatch actions.
  Initially passed holds remain passed until release. Injected input, fresh
  unowned combinations and the existing default Win+L exception are unchanged.
- Saturation drops action/trace evidence, not interception. A full queue evicts
  one oldest record to reserve necessary E8 mask evidence; losses are counted.
  The mask survives consumed holds and its actual send result is recorded.
  Bare swallowed repeats after Win-up do not rearm it, so a subsequent naked
  Win tap retains normal Start behavior. This follow-up regression was found
  during integration and fixed across every classifier arm.
- Fresh Shift state binds supported preheld move/send/sticky combinations;
  fresh Ctrl/Alt combinations remain unowned. The callback stays bounded with
  no synchronous logging, Engine work or window mutations.
- Maximize/fullscreen/float/sticky dispatch now rechecks suspension and elevated
  foreground per intent. The existing verified managed-fullscreen exemption
  preserves exiting project-owned fullscreen. Its native behavior needs live proof.
- Independent review exposed toggle fencing and coverage/contract gaps.
  Inspection additionally exposed modifier/Win guards ahead of armed repeats;
  one corrective unit fixed these before acceptance. Superseded managed-only
  assertions failed during migration and were updated to the authorized contract.
  No arbitrary missing-key-up timeout was accepted.

## Binding and conflict audit

- Reference: `kwin/src/plan-adapter-entry.ts` shortcut catalog; Windows
  `snapkey.rs::{catalog_index,push,push_digit,push_maximize,push_fullscreen,push_g}`.
- Before this change, directional, maximize, fullscreen, float/sticky and
  workspace-send interception depended on a managed origin. Workspace select
  did not, but all arms depended on tiling activity/fullscreen/elevation/gesture
  gates. Repeats/ups could pass when those conditions changed; saturation also
  deliberately passed. Those are all deliberate pass-through paths, now removed
  from enabled authentic interception.
- Conflicts below are documented in [Windows shortcuts][shortcuts], except
  Xbox-mode Win+F11, documented separately in [KB5070297][xbox-mode]. Availability
  depends on OS version, device and app. Absence from this list is not a guarantee
  against third-party shortcut owners.

| Our existing chord | KDE operation | Windows collision |
| --- | --- | --- |
| Win+Left/Right | Focus | Snap left/right |
| Win+Up/Down | Focus | Maximize/minimize |
| Win+Shift+Left/Right | Move | Move between monitors |
| Win+Shift+Up/Down | Move | Stretch vertically / restore snapped or maximized window |
| Win+H | Focus left | Voice dictation |
| Win+J | Focus down | Recall on supported devices |
| Win+K | Focus up | Cast / Connect |
| Win+L | Focus right, existing explicit opt-in only | Lock; no supported suppression established here |
| Win+Shift+H/J/K/L | Move | No matching default found in the cited Windows 11 list |
| Win+0..9 | Select workspace | Taskbar app launch/switch |
| Win+Shift+0..9 | Send and follow | Taskbar new instance |
| Win+M | Maximize toggle | Minimize all |
| Win+F11 | Fullscreen toggle | Enter/exit Xbox mode |
| Win+G | Float toggle | Game Bar |
| Win+Shift+G | Sticky toggle | No matching default found in the cited list |

- Win+Shift+M/F11 are not owned fresh chords. Extra Ctrl/Alt combinations,
  Win+Tab and Win+D remain OS-owned. Shift released during an already consumed
  Shift+L hold does not turn its repeats into an OS lock chord in the classifier;
  physical Windows behavior for this boundary remains unproven.

## User trace evidence (2026-10-03)

- Source: local `session-1/run-01dd530861947ef5.log`, 4081 lines, owner started
  with `--user-start --trace`, four ordinary windows including Notepad. Only
  targeted hook/toggle/lifecycle rows were extracted; no raw trace is committed.
- Lines 2113/2142: Win+F11 down/up consumed, tick 259 enters project-owned
  fullscreen and tick 261 closes its pair. The managed fullscreen is a retained
  overlay, not evidence of Xbox mode or whole-session suspension.
- Lines 2620/2636, ticks 304/305:

  ```json
  {"disposition":"passed","edge":"down","event":"fullscreen-toggle","outcome":"passed","tick":304}
  {"disposition":"passed","edge":"up","event":"fullscreen-toggle","outcome":"passed","tick":305}
  ```

- Both edges reached the hook and were deliberately passed. This establishes
  a product pass-through route, not an OS bypass of a consuming hook. The user
  physically observed Xbox-mode entry; the trace has no mode-specific signal.
  It does not identify the failing gate/origin term. No suspension was logged at
  304/305; unmanaged/null foreground, transient gesture/elevation or a transient
  foreground veto cannot be disambiguated from these rows.
- Line 2991: another consumed down at tick 337 restores project-owned fullscreen.
  No matching up row follows; its cause is unknown. Line 3002 is the run's only
  `suspend` (`fullscreen-foreground`), followed by `snap-stale dropped:1` and
  `resume`. A dropped owner record is not proof of a missed physical key-up.
- Win+M: 14 consumed down/up pairs, targets alternating maximized/restored, no
  passed maximize rows. Four `snap-mask` results insert both events successfully
  (one focus, two maximize, one fullscreen). `snap-available`, successful
  `snap-release` and `run-end status:stopped` bound the run.
- No float/sticky actions or logged repeats in this run. It cannot prove held
  repeat behavior, all release orderings, hook-presence continuity, or suppression
  by the new code. Existing unmaximise overlap remains a separate defect.

## Xbox detection blocker

- [Microsoft's Xbox-mode guide][xbox-mode] documents Win+F11, Game Bar, Task View
  and controller/touch entry/exit; one-window/fullscreen behavior; startup options;
  and disabled desktop shortcuts. It gives no developer runtime mode signal.
- Two bounded searches of Microsoft Learn/Support plus local prior art did not
  establish an official Xbox-mode-specific query/event. This is a search result,
  not proof that no API exists.
- Rejected substitutions: [SHQueryUserNotificationState][notification-state]
  reports notification suitability, exclusive Direct3D fullscreen and presentation
  mode, not Xbox mode; [Game Mode APIs][game-mode] concern game resources and are
  deprecated; [GameBar.Visible/IsInputRedirected][gamebar-ui] describe an overlay,
  not the full screen experience; [effective power GameMode][power-mode] is power
  policy; [Gaming Configuration unattend][gaming-unattend] is provisioning rather
  than runtime state. AppCapture/AppBroadcasting signals were not established as
  Xbox-mode detectors either.
- Cloaked ApplicationFrameWindow and DWM cloak 0->2 remain symptoms. No class,
  process-name, geometry or cloak heuristic was installed as mode detection.
- Options for the user: (A) retain the detection requirement and seek Microsoft
  clarification/a documented signal; (B) authorize a reversible explicit pause
  surface as an interim behavior, with manual enter/exit responsibility; (C)
  authorize heuristic research, accepting false pauses and missed transitions.
  Recommendation: A. B is a public-behavior decision; C changes the current
  documented-signal constraint. Automatic mode lifecycle is not implemented.

## Alternate Game Bar access (decision pending)

| Option | Evidence and consequences |
| --- | --- |
| Start-menu Game Bar launch | [Microsoft Learn][gamebar-launch] explicitly documents it; keyboard-only via Start/search, without changing our catalog. That guide cautions about PowerShell/Cmd focus. Needs current-host verification. |
| Controller Xbox button | [Xbox-mode guide][xbox-mode] documents it; no new tiler binding or input synthesis, but requires hardware. |
| User-configured Game Bar shortcut | [Xbox Support customization][gamebar-custom] and community Q&A describe it. Candidate for keyboard access; current Win11 UI, open-Game-Bar operation and reliability need verification. Q&A is not an API guarantee. |
| New tiler binding/tray action | Adds public behavior requiring user approval and a supported launch mechanism. No supported external main-Game-Bar activation was established. |

- Recommendation: approve documenting Start-menu/controller access now; evaluate
  the OS-custom-shortcut option for a convenient keyboard route before adding a
  tiler surface. No public binding, tray item or automatic launch was added.
- [Game Bar widget-control docs][gamebar-uri] describe `ms-gamebar:` URI syntax
  but explicitly say URI activation is currently unsupported apart from
  widget-to-widget activation. Do not treat ShellExecute/LaunchUriAsync of that
  URI as a supported external main-Game-Bar opener. `ms-gamebarservices:` has
  no suitable supported behavioral guarantee established here.
- Synthetic Win+G is not an alternate route: product ignores injected input,
  but synthesizing an OS gaming chord adds input behavior and anti-cheat exposure,
  and has not been authorized. Releasing Win+G in authentic mode would contradict
  the selected preset; it is not a technical fix for this change.

## Anti-cheat assessment

- No anti-cheat vendor certification, allowlist or specific incompatibility was
  established for this project or the mechanisms below. Compatibility and
  false-positive risk remain undetermined; official Win32 support and prior-art
  usage do not establish anti-cheat acceptance. No finding justifies a mechanism
  or architecture replacement without further evidence/user decision.

| Mechanism | Established facts and uncertainty |
| --- | --- |
| WH_KEYBOARD_LL | [Microsoft][ll-hook] says callbacks run in the installer's context, without injection into the target process. A prompt message loop is required. That differs from a game DLL hook, but is not anti-cheat certification. |
| E8 SendInput mask / focus prime | [SendInput][send-input] is documented and UIPI-limited. It creates detectable injected events; use is bounded and is not a demonstrated anti-cheat-safe exception. |
| Topmost/layered border and underlay | Separate project-owned windows, no game rendering injection. Existing carriers/band logic are in `active_border_sys.rs`, with eligibility in `active_border.rs` and `group_underlay.rs`; topmost/layered API support does not guarantee game/anti-cheat compatibility. |
| Cross-process window placement | Documented [SetWindowPos][set-window-pos], with existing identity/integrity/scope fencing. Fullscreen/protected targets are refused, not forcibly overridden. Vendor compatibility remains unknown. |
| Workspace hide / cloaking | Product workspaces use public [ShowWindow][show-window] SW_HIDE with recovery; DWM cloak observation is not Xbox detection. Prior art also uses COM virtual-desktop cloaking. [DWM cloak attributes][dwm-attributes] are documented, but cross-app use/anti-cheat support cannot be inferred from that alone. |

- Reliability boundary: Microsoft documents that LL-hook timeout can pass an
  event and silently remove the hook, with no supported notification of removal.
  This correction removes deliberate pass-through while the hook is present; it
  cannot promise absolute OS suppression after hook loss or on secure desktops.
  If physical Win+F11 still enters Xbox mode despite logged consume/mask success,
  stop and return the evidence/options; do not add registry, driver or injection
  workarounds. A dedicated-thread architecture or broader gaming exclusions would
  require a separately justified user decision.

## Local prior art

- Revisions below were read-only inspections; source paths are relative to each
  repository under the user's Development directory. Searches of their relevant
  source found no explicit Game Bar/Xbox full screen experience or anti-cheat
  compatibility contract. These are search-limited absences, not certifications.

| Repository / revision | Relevant source and behavior |
| --- | --- |
| GlazeWM `5709ad0a3c7c386bbc3e38166a865ffc12937515` | `packages/wm-platform/src/platform_impl/windows/keyboard_hook.rs`: WH_KEYBOARD_LL. `native_window.rs`: SetWindowPos, ShowWindowAsync, COM `set_cloak`, focus priming, MarkFullscreenWindow. `packages/wm/src/traits/window_getters.rs`: fullscreen frame-cover check. `wm.rs` / `wm_state.rs`: explicit pause and ignore commands. |
| komorebi `e0709f02bfae4e503bf4640f58ee75ecbbfdbb97` | `komorebi/src/com/mod.rs` and `com/interfaces.rs`: virtual-desktop cloak. `windows_api.rs`: DWM cloak reads. `window.rs` / `winevent.rs`: cloak/uncloak lifecycle. `core/asc.rs`: ignore/float/layered rules. `process_event.rs` / `process_command.rs`: pause. Shortcuts delegated to whkd/AHK; no in-process LL keyboard hook found. |
| Seelen-UI `56c1d75dae814bd5c3d03d5eea3d6f0428b02db1` | `Cargo.toml`: win-hotkeys keyboard hook. `background/windows_api/mod.rs`: fullscreen geometry and service-mediated window operations. `background/state/application/performance.rs`: fullscreen or GameMode/MixedReality power mode selects Extreme performance. `modules/power/*`: effective power signal and widget suspension. `cli/shortcuts.rs`: shortcut pause. This is generic performance policy, not Xbox detection. Its system-tray WH_CALLWNDPROC DLL is a different mechanism from our LL hook. |
| Whim `1e86b579206373a8e9939a5343b67696419ed284` | `src/Whim/Keybind/KeybindHook.cs`: WH_KEYBOARD_LL. `Store/WindowSector/Transforms/WindowAddedTransform.cs`: cloaked-window exclusion. `Native/CoreNativeManager.cs`: cloak query. `Processors/FirefoxWindowProcessor.cs`: cloak sequencing. `Filter/DefaultFilteredWindowsKomorebi.g.cs`: imported app exclusion rules. No mode-specific solution found. |

## Verification and required live plan

- Baseline: `de213b2`, clean working tree, aligned with `origin/main`.
- Native gates: the four packages in `.github/workflows/ci.yml` Windows job.
- Latest native verification passed after the corrective unit and gate-repeat
  integration: locked build/test for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`; strict all-target clippy; full rustfmt;
  `git diff --check`. Snapkey suite: 55 passing, including modifier/Win ordering,
  unmanaged interception, disabled-gate pairs and saturated queues. Three pure
  Shift-sync tests pass. First delivered unit `962b0f3` passed all four hosted
  jobs (Windows, Rust, KWin, shell), [run 37109086579][first-ci]. The follow-up
  mask correction passed native gates; delivered-head CI is checked after
  publication.
- No live run was performed for this change. Required normal-desktop test, only
  after the user frees the desktop and authorizes the bounded run:
  1. Read the live guide, confirm taskbar/normal desktop, no game/anti-cheat or
     protected target. Record source/artifact hash, exact owner identity, display
     and restoration baselines, including SPI 0x0082=1 and pen 0x201E=35. Preserve
     the hosting Terminal process/tree and establish the out-of-hook stop route.
  2. Open four disposable Notepad windows. Run
     `just tile --user-start --trace --scope-exe notepad.exe`
     through the approved launcher. Confirm owner-ready and `snap-available`
     before any gaming chord. Terminal and other executables remain unmanaged.
  3. Physical taps and bounded two-second holds: arrows/H/J/K, Shift+direction,
     0..9 select/send, M, F11, G and Shift+G. Repeat on managed Notepad, unmanaged
     Terminal, Notepad's unadmitted dialog and desktop foreground. No Win+L lock
     or elevated/secure-desktop test. Windows must not Snap/minimize-all/open
     dictation/Cast/Recall/taskbar apps/Game Bar/Xbox mode for owned chords.
  4. On an owned chord hold, vary Shift/Ctrl/Alt and release Win before the chord,
     then test the reverse release ordering. Expect consumed pairs, no toggle
     repeats, no actions after the live modifier combination changes, successful
     bounded `snap-mask`, and no accidental Start. Plain Win, Win+D/Win+Tab and
     fresh Win+Shift+M remain OS-owned. Prehold Shift across hook install in one
     separately bounded restart to verify move/send/sticky selection.
  5. Enter/exit project-owned fullscreen with physical Win+F11 twice; both must
     log consumed down, fullscreen/restored, and preserve the normal desktop.
     Verify sibling workspace state after physical workspace select/send.
     Any unexpected Xbox entry ends the run: user uses Task View's desktop
     return surface and the out-of-hook exact-owner stop, not more injected keys.
  6. Graceful `just stop` and restore; verify released
     hook/overlays, revealed windows, no project owner/watcher/ledger residue,
     SPI and pen baselines. If needed use exact-identity emergency-stop then
     restore per the guide. Close only newly opened disposable windows.
- Expected current logs: consumed `snap`/`workspace`/toggle records or settled
  origin/suspended/elevated rejections, `snap-mask result:mask-ok`, no owned
  `disposition:passed` while handling is enabled, successful `snap-release` and
  `run-end status:stopped`. Blocked batches may use bounded `snap-stale` instead
  of per-chord records. Trace absence alone does not prove OS suppression.
- Xbox lifecycle testing is deferred until a documented signal/approved route
  exists. Its acceptance must include startup-in-mode, enter, exit, new/closed
  windows and workspace continuity; bounded enter/exit/restore outcomes; no
  geometry/effects/shortcut consumption in mode; exact normal-desktop cleanup.
- No new public behavior or provisional product decision was selected. Next:
  user decision on detection/access options and normal-desktop physical proof.

[shortcuts]: https://support.microsoft.com/en-us/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
[xbox-mode]: https://support.microsoft.com/en-gb/topic/windows-gaming-full-screen-experience-67fb8d12-5467-4a95-8adf-0a10789576ab
[gamebar-launch]: https://learn.microsoft.com/en-us/xbox/game-bar/quickstart/run-gamebar
[gamebar-custom]: https://support.xbox.com/en-US/help/games-apps/game-setup-and-play/customize-game-bar-on-windows-10
[gamebar-uri]: https://learn.microsoft.com/en-us/xbox/game-bar/api/xgb-widgetcontrol
[gamebar-ui]: https://learn.microsoft.com/en-us/uwp/api/windows.gaming.ui.gamebar
[notification-state]: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state
[game-mode]: https://learn.microsoft.com/en-us/previous-versions/windows/desktop/gamemode/game-mode-portal
[power-mode]: https://learn.microsoft.com/en-us/windows/win32/api/powersetting/ne-powersetting-effective_power_mode
[gaming-unattend]: https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-gaming-configuration
[ll-hook]: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
[send-input]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
[set-window-pos]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos
[show-window]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow
[dwm-attributes]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
[first-ci]: https://github.com/beefsack/OmniTiler/actions/runs/37109086579
[diagnostic-ci]: https://github.com/beefsack/OmniTiler/actions/runs/37113756434
[hook-order]: https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks
[register-hotkey]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey
[game-shortcut-hook]: https://learn.microsoft.com/en-us/windows/win32/dxtecharts/disabling-shortcut-keys-in-games
