# tiler-macos — scoped tiling preview and capability probes

Tier 2 (public + scoped private APIs, SIP left enabled). Apple Silicon only;
minimum macOS 15. All Rust; `unsafe` exists
only in `src/ffi.rs` (single dedicated module); every other module carries
`#![forbid(unsafe_code)]`. Trusted `objc2` (`application-services`, `core-foundation`, `app-kit`,
`foundation`), `core-foundation`/`core-graphics`, and `libc` wrappers are
used; the sole hand-written declaration is the private
`_AXUIElementGetWindow` join. Dynamic Carbon/SkyLight tables are resolved via
`libc` with null-checked signatures. Calling maintained wrappers still needs
isolated `unsafe` at each call site. The declaration surface is small; the
safe-wrapper/stub module remains substantial.
No foreign pointer crosses the FFI boundary: callers get owned
opaque handles (`AxElement`), plain data, or errors.

Threading: main thread AppKit/event dispatch; dedicated input owner for scoped
Carbon registrations or the explicit event tap with a minimal callback;
standalone Carbon probes retain their main-thread pump. Per-app AX threads have
finite messaging timeouts and bounded runloops; engine thread owns state via
a bounded `sync_channel` (`try_send` backpressure, never a block in a
callback).

## Scoped tiling development mode

The scoped preview connects native AX facts and current-Space display domains
to the shared Engine, with per-app frame writes/readback and owner-dispatched
SLPS focus. **Basic scoped tiling is live-proven with explicit tap input**;
physical Carbon delivery and OS timeout-notification recovery remain pending.
See the
[change record](../../docs/changes/macos-scoped-tile.md) and
[live protocol](../../docs/live-macos-testing.md).

```zsh
mise exec -- just app 'OmniTiler Mac Dev'
mise exec -- just verify
mise exec -- just preflight-app
mise exec -- just tile --user-start --scope com.apple.TextEdit --scope com.apple.calculator --input tap --seconds 120
```

`--user-start`, a nonempty bundle allowlist and a finite 1–120 second lifetime
are mandatory. `--input carbon` is the default; `--input tap` explicitly
installs exact-chord interception; `--input off` disables input. `--modifier
ctrl|meta|alt|shift` configures the modifier (Ctrl by default; `meta` means
Command). Focus uses modifier+H/J/K/L or arrows; move adds Shift; float toggle
is modifier+G. Carbon conflicts are reported per chord; no native settings
are changed. Shift alone cannot distinguish the focus/move arms and is
rejected for this catalogue.

The separately signed `OmniTilerProbeHelper.app` has bundle ID
`io.github.omnitiler.macos.probe.helper`; it can join an explicit helper scope
when launched through LaunchServices with its bounded nonce arguments. The
directly spawned, unbundled probe helper remains a separate ownership route.
Own-probe, Ghostty, protected-terminal and unknown bundles never actuate in
scoped mode. Scope is app-wide, so all windows of a named app are candidates.

Only each display's current ordinary user Space participates. Fullscreen,
minimized, hidden, ambiguous-membership and out-of-scope windows take no
geometry/focus writes. Display/Space changes supersede plans; unreadable
topology invalidates them until a fresh snapshot recovers. The session
dictionary's typed screen-lock marker, login/console state and Secure Input
gate native effects and input; unknown lock state refuses.
Input guards release outside the explicit foreground scope and in fullscreen;
AX-owner-published facts gate callbacks without running AX on the input thread.
Calculator's native fixed-size clamp is currently tolerated rather than
invented as equal min/max hints; its slot can remain larger than its frame.

Normal lifetime expiry performs bounded shutdown and releases input; stop or
process loss leaves window geometry in place. An independent watchdog forces
a dirty exit after startup allowance + requested lifetime + 5 seconds teardown
+ 30 seconds grace. Watchdog expiry reports stderr rather than terminal JSON.
The LaunchServices wrapper polls completion and cannot prove the app's exit
code: inspect the JSON and structured diagnostics. Run build/test gates
sequentially when they share the Cargo target directory; the signed build
sets a deployment target that plain Cargo does not.

## Owned target

AX behavior runs only against the owned disposable AppKit probe window in a
SEPARATE process (`tiler-macos-probe-target --nonce HEX …`). `probe-spawn`
returns only after the child proves itself on stdout (bundle + exact PID +
nonce) AND prints a `ready` marker after the runloop services its window
(early exit or either timeout kills the exact child and fails); `probe-stop`
reloads the spawn record, requires nonce equality, and revalidates the live
executable path plus the CURRENT live window nonce (owner PID + exact nonce
title) before signalling (PID-reuse guard).
Every mutating path rechecks the live bundle table first. Never Ghostty, the
controlling terminal, or arbitrary windows.

Current ownership: the exact spawned `Child` handle plus its PID, the
per-run nonce over the child's own stdout pipe and in its window title, the
recorded executable path, and a CURRENT live binding on every cross-command
use (fresh CGWindowList snapshot must show a window owned by exactly that
PID with exactly the recorded nonce title). No `NSRunningApplication.launchDate`
token is used by these direct-Child probes; the scoped tiling adapter separately
binds LaunchServices app lifetimes. Live status:
window-server titles (`kCGWindowName`) are withheld without Screen
Recording consent (2 of 44 entries named system-wide; the owned helper's
title is absent), so cross-command stop/read/write fail closed with a
nonce-unprovable refusal until a live title channel exists; where CG titles
are withheld, a minimal owned AX title fallback applies only with
Accessibility trust actually observed true (bounded worker over CG-joined
owned windows, exact nonce-title compare, titles never logged) and otherwise
fails closed — trust-false is reported as a consent shortfall, never as a
SIP rejection; the
`probe-spawn` holder path (exact `Child` handle, no cross-command auth)
is unaffected. The bounded `spaces-session` holder extends the same
contract: it retains the exact `Child` end to end, proves graceful/kill
first, then verifies pipe nonce, `try_wait` liveness, exe binding, and
fresh CG owner-PID joins at each action while running CG/Space
read/permission/topology readbacks inside the holder — never cross-command
title auth (observed neutrally live mid-session, never depended on). The
holder dispatches no Space writes: independent restore is not yet proven,
so `--allow-untested` does not authorize mutation. Authenticated CG/Space/AX runs and successful-session
graceful stop are proven live only when the evidence below says so; see the
[Phase 0 handover](../../docs/changes/macos-phase-0.md).

## CLI contract

`help` and `check`/`preflight` (default with no args) never launch UI, hooks,
taps, AX control, or Space moves:

```zsh
mise exec -- cargo run --locked -p tiler-macos -- help
mise exec -- cargo run --locked -p tiler-macos -- preflight
mise exec -- just launch -- ax-enumerate
mise exec -- cargo run --locked -p tiler-macos -- secure-input
mise exec -- cargo run --locked -p tiler-macos -- spaces-diagnose
mise exec -- just launch -- cg-owned-windows --pid PID --nonce HEX
```

TCC attribution: a directly executed binary (`cargo run`, or the executable
path inside the `.app`) runs under the terminal's responsible-process
attribution, so `ax_trusted:false` there is expected and is not a consent
failure. Every TCC-dependent probe (anything reading AX trust, observing,
or mutating owned windows) must instead run through LaunchServices so the
window server attributes it to the signed bundle, via the robust
poll-for-output route (`open -W` cannot block on this bundle):

```zsh
mise exec -- just preflight-app
mise exec -- just launch -- ax-enumerate
mise exec -- just launch -- --timeout 60 -- spaces-session --seconds 20
```

Status honesty: the wrapper observes output files and process quiescence —
it cannot see the probe's real exit code (`open` never reports it). Read the
JSON payload (`verified_exit`/`restored`/`effect_verified`) as the outcome;
a wrapper timeout or any stderr content is never success evidence, even with
partial stdout beside it.

Explicit bounded modes (all finite; `--ctrl meta|ctrl|alt|shift`, default
`ctrl`; fixed probe chord `0x04`):

```zsh
mise exec -- just launch -- ax-observe --pid PID --nonce HEX --timeout-ms 1000
mise exec -- just launch -- input-register --seconds 10 --ctrl ctrl
mise exec -- just launch -- input-tap --seconds 10 --ctrl ctrl
mise exec -- just launch -- input-tap --seconds 10 --ctrl ctrl --suppress
mise exec -- just launch -- spaces-diagnose --wid WID [--pid PID --nonce HEX]
mise exec -- just launch -- --timeout 120 -- spaces-move --pid PID --nonce HEX --wid WID --space ID --allow-untested
mise exec -- just launch -- --timeout 120 -- spaces-restore --pid PID --nonce HEX --wid WID --space ID --allow-untested
mise exec -- just launch -- --timeout 45 -- probe-spawn --seconds 30
mise exec -- just launch -- --timeout 120 -- spaces-session [--seconds N] [--allow-untested]
mise exec -- just launch -- --timeout 120 -- ax-session --seconds 30
mise exec -- just launch -- --timeout 120 -- focus-session --seconds 30
```

`focus-session` is the Child-holder SLPS focus session over the opt-in
regular-policy fixture: it captures the original workspace+AX+CG preimage
with executable currency, unfocuses via the SAME route first when the spawn
already holds agreement (non-vacuous proof), dispatches the yabai SLPS route
(`_SLPSSetFrontProcessWithOptions` + two `SLPSPostEventRecordTo`
key-window posts) to the owned window with workspace+AX+CG agreement, then
restores the original via the SAME route with workspace+AX+CG agreement,
per-display Space unchanged, and verified exact-Child cleanup. Every
post-spawn failure attempts the SAME-route restore subject to original
identity currency before cleanup, recording a structured restore attempt
(dispatch/agreement outcomes with causes; null only when currency refusal
meant nothing was attempted, with the reason recorded). No activation API,
taps, keys, SA, or Space writes. The regular-policy fixture is experimental,
live-proven with the selected SLPS/NSWorkspace/direct-app-AX/CG oracle. The
main polling loop services AppKit/run-loop work while waiting; sleeping there
previously left workspace foreground evidence stale.

`focus-diagnose` is the bounded read-only focus-evidence diagnostic: raw
systemwide AX status beside direct frontmost-app count/focused/main reads,
before/after `AXFrontmost`, and CG ownership. It emits IDs/statuses and timing,
never titles/content, with a 60-second hard lifetime. The default uses raw
initialization; `--appkit` initializes/pumps an Accessory app like tile mode.
`--fallback` explicitly evaluates unaccepted SLPS-front corroboration for
diagnosis only. Production and acceptance use SLPS front-process PID agreeing
with NSWorkspace and the direct per-app AXFocusedWindow (AXMainWindow fallback)
joined to CG ownership; systemwide AX and AXFrontmost never gate. The
same optional `--appkit` initialization discriminator exists for `focus-session`.
Current evidence and unresolved initialization/query-order confounds are in
the [scoped-tile record](../../docs/changes/macos-scoped-tile.md).

`scope-inspect --scope BUNDLE` reads bounded, title-free native facts,
classification, AX/CG frames and membership without actuating windows.
`input-session --route tap --direct-rearm` is the bounded owned-helper native
suppression/passthrough/re-arm fixture; its results are synthetic pipeline
evidence, not physical keyboard evidence. `--watchdog-only` runs an explicit
hard-exit fixture and is not a normal tiling command.

```zsh
mise exec -- just launch -- --timeout 75 -- focus-diagnose --appkit
```

`ax-session` is the Child-holder AX session: it proves focus restoration on
a throwaway spawn first, then runs pre-window plus window-joined observers
with a driven move/resize, owned self-activation, raise with focus readback,
frontmost restore, and exact-Child stop — then a fixed-size phase proving
controlled partial-write (position accepted, size refused) restoration. No
cross-command title auth inside; per-leg in-process latencies in the report.
The responsive phase additionally self-closes the owned window on a timer
while the joined observer pumps, proving `AXUIElementDestroyed` delivery with
on-screen CG absence plus process-alive readbacks before the exact-Child stop.
Offscreen CG entries can linger until app exit and are reported explicitly;
the final stop must separately prove no CG residue.

`ax-observe` authenticates the owned session (spawn record: pid/nonce,
expected helper, live bundle-or-exe plus CURRENT live window nonce) and
refuses arbitrary, protected,
or foreign PIDs before any AX call. `input-tap` defaults to observe-only
(`Keep` always); `--suppress` arms exact-chord `KeyDown`/`KeyUp` pair
suppression only — unrelated keys, inexact modifiers, lone ups, and
`FlagsChanged` always pass, and the engine receives exact-chord matches
only, never raw deliveries. `spaces-diagnose --wid` adds a read-only
per-window Space membership read via the pinned yabai
`SLSCopySpacesForWindows(cid, 0x7, [wid])` shape (IDs only). With
`--pid/--nonce` the WID is first joined to the owned PID through a fresh
CGWindowList snapshot (no AX) and the output is labeled `authenticated`;
without identity the read is labeled unauthenticated (IDs only, no PII).
`cg-owned-windows` lists the owned PID's windows (WID, layer, on-screen
flag, bounds) from the public `CGWindowListCopyWindowInfo` snapshot via the
maintained `core-graphics` 0.25.0 binding. The snapshot itself needs no AX;
cross-command nonce authentication may use the Accessibility-gated AX-title
fallback when CG titles are withheld.

Every success report carries measured effects (OS status codes, notification
counts, frame preimage/readback/restore, tap deliveries, re-enable events).
`ax-set-frame` captures a preimage, sets, reads back, restores, and verifies
the restore on the successful path. Partial position/size write failures
trigger exact-preimage restoration with structured failure/restoration
evidence (pure fault-injection regression tests cover the set, readback,
restore-write, restore-verify, and ownership legs on all platforms). Full and
controlled partial-write restoration also have owned-fixture live evidence in
the [Phase 0 record](../../docs/changes/macos-phase-0.md). The caller waits a
documented 75 s worst case (5
write-boundary revalidations × 8 s AX-title fallback + 5 frame legs × 2
calls × 2 s element messaging timeout + 15 s slack, worker never
cancelled); a timeout means outcome unknown with a restore possibly still in
flight — never clean, never complete.
`ax-raise` captures the exact focus preimage first and refuses before
changing focus unless the preimage is already the owned session (never
steals focus from the terminal or foreign apps); success requires a focus
readback equal to the target, never OS acceptance alone, and a missed
readback re-raises only the owned preimage window. The probe window itself
is `Accessory` + `orderFront:` (never key), so spawn steals no focus.
`spaces-move` is a direct-`SLSMoveWindowsToManagedSpace` owned-move probe
(experimental: one gated dispatch on an owned fixture observed no effect):
it authenticates the full spawn record
(pid/nonce/exe plus CURRENT live window nonce — never identity claims
alone), joins the WID to the
owned PID via CGWindowList with a live recheck (no AX), captures the
membership preimage (exactly one Space required; sticky multi-Space
membership fails closed) plus the CG geometry/visibility baseline, validates
the target against enumerated Spaces, requires the explicit
`--allow-untested` prerequisite per invocation, revalidates the record
unchanged immediately before the write and again before the effect readback,
then dispatches and reads back the effect. The report separates the void
dispatch (route + symbol availability) from the effect (post-move membership
with exact-equality verification: the effect must equal exactly the one
target Space, not merely contain it); a missing second Space is reported as
an explicit topology block, never as a SIP failure, and no SIP rejection is
asserted without live evidence. The compat-ID route is disabled as a
mutation (`SLSSpaceSetCompatID` mutates global Space metadata with only a
setter covered, no exact preimage, and a yabai-colliding tag — reset to zero
is not restoration); its symbols stay presence diagnostics only. The move
never auto-restores — `spaces-restore` performs the inverse with the same
full validation (usable after the mover exits, via the spawn record). The
single-shot direct inverse is unverified; the separate public AppKit
held-Child recovery (`spaces-owned-recovery` O->T->O) is proven per run.
Source and destination must read back as user Spaces; unreadable Space type,
unknown frontmost context and loginwindow fail closed before dispatch. A
frontmost observation is not an atomic lock detector. Source and target must
map unambiguously to the same display; cross-display recovery remains unproven.
`spaces-switch`, `spaces-create` and `spaces-destroy` stay explicitly
unsupported single-shot commands: their non-SA single-shot routes are
unimplemented here (see the [route survey](../../docs/research/macos-port/native-spaces-routes.md))
and the commands report the route instead of executing — use the bounded
gated probes below, which carry their own per-run readbacks.

Native-Space write routes (source-derived surveyed route at pinned yabai
`dd845723416f5fe92af49fad5ebab00369e07edd`, not a universal claim; no SIP
rejection asserted without live evidence):

| Operation | Tier-2 route | Scripting-addition only? |
| --- | --- | --- |
| Window move | Direct `SLSMoveWindowsToManagedSpace` only (experimental: one gated dispatch observed no-effect on an owned fixture, possibly ineffective on the macOS 15 floor; single-shot inverse unverified, separate public held-Child recovery proven); compat-ID fallback disabled (no exact restoration); bridged async op detected, never executed ([space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705), [extern.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L63-L63)) | No (SA attempt omitted) |
| Switch vs focus | Yabai non-SA gesture fallback; AX Mission Control and enabled system chords are additional candidates | No; single-shot unimplemented here — use `spaces-switch-probe` (gesture legs read back the current both ways; arrow session pair observed no effect) |
| Create / destroy | Yabai commands use SA; Hammerspoon uses non-SA AX Mission Control | Yabai yes; AX MC create/goto/remove proven on one temporary same-display Space — the integrated `mc-lifecycle` run is NOT passed (first run failed on the immediate goto; only the separate corrected recovery+remove passed; see below) |
| Reorder / Space-to-display move | Yabai SA internal Dock functions | Yes in surveyed yabai; no proven non-SA implementation |

## Mission Control + Space switch/recovery probes (bounded, gated)

`mc-inspect` is read-only (session lock, AX trust, Dock identity, MC-group
presence, sanitized topology; no launch, no writes, no synthetic events).
`mc-lifecycle [--seconds N] [--display INDEX] --allow-untested` is the
bounded temporary-Space lifecycle: create exactly one Space on the display,
go to it and back, remove ONLY the created Space. No window is ever targeted
(no close/quit/minimize/hide/move/resize/Space-move of any window) and no
synthetic event is posted on any path in the module.

```zsh
mise exec -- just launch -- mc-inspect
mise exec -- just launch -- --timeout 120 -- mc-lifecycle --seconds 60 --display 0 --allow-untested
mise exec -- just launch -- mc-members --space SID
mise exec -- just launch -- spaces-bindings
mise exec -- just launch -- --timeout 120 -- spaces-owned-recovery --original SID --temp SID --allow-untested [--probe-direct]
mise exec -- just launch -- --timeout 120 -- spaces-switch-probe --original SID --temp SID --allow-untested
mise exec -- just launch -- --timeout 120 -- mc-recover --temp SID --original SID --allow-untested
mise exec -- just launch -- --timeout 120 -- mc-restore-frontmost --pid PID --wid WID --space SID --allow-untested
```

Route (Hammerspoon public-AX shape, private toggle replaced): the open/close
toggle is the public `NSWorkspace.launchApplication("Mission Control")`
(no Accessibility trust, no Automation consent, no events posted); traversal
is direct per-app AX on the exact `com.apple.dock` PID (`mc` →
`mc.display` joined by `AXDisplayID` via the AppKit display snapshot →
`mc.spaces` → `mc.spaces.add` press to create, `mc.spaces.list[i]` press to
go to a Space, `AXRemoveDesktop` to remove one), per
([spaces.lua](https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L84-L152)).
The AX legs still require observed Accessibility trust. Every action
re-checks the `CGSessionCopyCurrentDictionary` unlocked verdict and AX trust
immediately beforehand and re-joins SID/display/index fresh; the temp SID is
adopted only through the exactly-one-new-SID gate (preexisting order and
currents intact); removal runs only on a uniquely located, inactive,
non-last temp, and ownership uncertainty blocks removal (residue plus
preimages reported, never a blind delete). A bounded `caffeinate -d -t`
Child rides the run and is reaped by exact handle on every path; currents
restore ordered per display and the original frontmost bundle is read back
(natural return evidence only — never repaired by focus writes here).
Visual Mission Control transitions are unavoidable on this route; observing
jank is user-owned.

Gated companions: `mc-members --space SID` is read-only member inspection
(every live member of the Space with PID/WID/bundle/layer/onscreen/bounds/
membership plus focus-oracle legs and the Ghostty snapshot; IDs and geometry
only); `spaces-bindings` is the read-only effective symbolic-hotkey table
(chord + enabled, no symbolic IDs, plus Ctrl+Left/Right corroboration);
`spaces-owned-recovery` is the bounded public O->T->O battery over two
existing SIDs (exact membership plus exact frame restoration, helper stopped
exactly; creates/destroys no Space, no SLS move, no synthetic posts;
`--probe-direct` adds ONE gated direct-SLS move in a separate short-lived
mover child with delayed readbacks plus independent public restore);
`spaces-switch-probe` is the bounded same-display arrow-vs-gesture
comparison over two existing SIDs (two owned helpers bracket enabled
Ctrl+Right/Left posts and the yabai began/ended gesture pair; creates/
destroys no Space, no SLS move, no tap input; observed standing: gesture
legs read back the current both ways, the arrow session pair observed no
effect); `mc-recover` adopts ONLY the
exact ordered [original, temp] topology and removes ONLY that temp when the
strict emptiness gate already proves clear (else parked, never clean);
`mc-restore-frontmost` is a guarded Ghostty-only single app activation with
current-Space guard plus full oracle agreement and stability readback.

Evidence boundaries (all gated probes): per-run readbacks are the only
effect evidence; switch `*-post` latencies cover owned-focus plus the
synthetic post only (current-readback settle polling excluded, never
end-to-end); the single-shot direct SLS move is experimental (observed
no-effect on one owned fixture) with its single-shot inverse unverified —
the separate public held-Child recovery is proven per run; AX Mission
Control create/goto/remove is proven on one temporary same-display Space,
but the integrated `mc-lifecycle` run is NOT passed (its first run failed
on the immediate goto; only the separate corrected recovery+remove passed);
the public `NSWorkspace` Mission Control close semantics were observed per
run only — other apps and releases are not proven; no SIP rejection is
asserted without live evidence; no product-workspace claim and no universal
native window-move failure claim is made. Canonical live verdict:
[docs/research/macos-port/native-spaces-routes.md#live-verdict-2026-10-16](../../docs/research/macos-port/native-spaces-routes.md#live-verdict-2026-10-16)
plus the [Phase 0 record](../../docs/changes/macos-phase-0.md). This unit
is offline verification only — no new live numbers.

## Signed app recipe (stable identity)

Fixed bundle ID `io.github.omnitiler.macos.probe`, fixed output path
(`target/macos/OmniTilerProbe.app`), one persistent self-signed dev identity
matched exactly by full name (never a substring), no paid signer, no ad-hoc
fallback, no custom output path. Deployment target macOS 15 is enforced by
the build script's `MACOSX_DEPLOYMENT_TARGET` export plus a `verify`-time
`LC_BUILD_VERSION` minos check — a plain `cargo build` outside the script
does not enforce the floor and fails closed at verify. The bundle must
contain no symlinks (foreign-file guard inside the fixed target). Cert
creation uses Certificate Assistant as described in the setup runbook; the
script fails closed until a persistent identity exists:

```zsh
mise exec -- just app 'OmniTiler Mac Dev'
mise exec -- just verify
```

Confirm the bundle grant through the LaunchServices route (never the bare
executable path): `mise exec -- just preflight-app` must report
`ax_trusted:true` before any TCC-dependent leg. A rebuild must preserve the
grant (stable designated requirement across rebuilds); if trust is lost,
stop — no TCC UI scripting and no `tccutil reset` in this flow.

The script builds both binaries, assembles the `.app`, signs nested
executables first and the bundle last, then runs `codesign --verify --deep
--strict`, prints the designated requirement (`codesign -d -r -`), and
reports a `spctl` diagnostic. It selects one exact-name match from the
**Matching identities** section of `security find-identity -p codesigning`,
regardless of trust, and signs by that identity's SHA-1. Zero or multiple
matches are hard errors. Self-signed identities do not need **Always Trust**;
private-key access authorization is separate.

## Live preflight / recovery

Preflight (nonmutating): record source revision, artifact path + SHA-256,
bundle ID, signing identity + designated requirement, launch args, session;
baseline display topology, Space membership, and for each owned window its
PID, `_AXUIElementGetWindow` join, CG window ID, geometry, visibility, Space,
and focus; prove graceful-stop and exact-identity kill on a non-tapping
process first; keep physical input + Force Quit + Activity Monitor + an
out-of-tap route available.

Recovery ladder: graceful stop → exact-identity terminate (revalidated
executable/bundle/PID/start) → independently proven restore → stop and ask
the user for the out-of-tap route. Never kill by broad name.

Artifacts (generic, no host paths in repo): the fixed-path `.app` under the
build tree, `preflight.json`, tap/AX lifecycle summaries only (no keystroke,
title, or content logs). Stop interception when Secure Input is active
(`secure-input` reports `Active|Inactive|Unknown`; unknown is not inactive).

## Gaps (honest, Phase 0)

- Hand-written FFI is one private declaration (`_AXUIElementGetWindow`) plus
  null-checked dynamic Carbon/SkyLight tables (Tier-2 move diagnostics plus
  the direct `SLSMoveWindowsToManagedSpace` write; compat setters have no
  callable wrapper; public `CGWindowListCopyWindowInfo` via the maintained
  `core-graphics` binding); maintained-wrapper calls still
  need isolated `unsafe`; the wrapper/stub module is not yet tiny. See the active
  [Phase 0 record](../../docs/changes/macos-phase-0.md).
- Owned Space move/restore: direct-only implementation, experimental live
  (one gated dispatch observed no-effect on an owned fixture with exact
  readbacks; single-shot inverse unverified while the separate public
  held-Child recovery is proven per run; `--allow-untested` required per
  invocation). Single-shot switch/create/destroy: explicitly unsupported;
  the single-shot commands report the pinned route instead of executing —
  use the bounded gated probes (`spaces-switch-probe`,
  `spaces-owned-recovery`, `mc-lifecycle`/`mc-recover`, `mc-members`,
  `mc-restore-frontmost`, `spaces-bindings`), which carry their own
  gates and per-run readbacks; they are not product workspaces and prove no
  universal native-move outcome.
- Nonces are OS-CSPRNG (`/dev/urandom`, std only, no new crate); spawn
  records live in a `0700` UID-owned session dir as `0600` `create_new`
  files with symlink/stale/replaced-identity refusal, and the recorded exe
  is constrained to the expected helper. Ownership is the exact `Child`
  PID plus the pipe-proven nonce plus the recorded executable plus the
  CURRENT live window-nonce title (fresh CGWindowList owner-PID join with
  exact nonce title on every cross-command use; no process-identity
  machinery, no `launchDate` token); a stale record plus a same-executable
  PID reuse shows a different nonce title, or none, and is refused. An
  unprovable title (window server withholds names) fails closed. Live direct
  spawn binds this way, unblocking authenticated owned probes.
- Carbon delivery needs a live user keypress on the main-thread runloop;
  `os_accepted` proves registration only.
- Event-tap creation needs Accessibility consent for the stable bundle
  identity; denial surfaces with the grant path, not a bypass. Suppression
  (`--suppress`) is implemented for exact-chord pairs but UNVERIFIED live:
  no suppressed-delivery probe has run on a Mac yet.
- `ax-raise` focus policy is fail-closed by design: it refuses whenever the
  current focus is not already the owned probe, so live use needs the user
  to focus the owned window first. Preimage readback needs AX consent.
- Per-window Space membership (`SLSCopySpacesForWindows` shape pinned from
  yabai source, read-only) has its first live proof via the `spaces-session`
  holder (preimage + post readback on the owned window, no Space write, no
  Space effect tested); cross-command owned membership still fails closed
  on withheld titles.
- TCC grant survival across rebuilds: unverified (needs stable-identity live
  run).
- Tap release-on-disable/exit + physical delivery: must be proven live before
  broader tap use. Timeout-disable is not recovery evidence.
- `probe-stop` without a spawn record is refused, as is any cross-command
  use whose live window nonce cannot be proved. Child-handle sessions
  (`probe-spawn`
  holding the exact `Child` until stop, and the read-only `spaces-session`
  holder with lifecycle proofs plus CG/Space/permission readbacks) remain
  safer than cross-command
  PID control; the record path supports bounded cross-command probes.
- `ax-set-frame` geometry is a fixed Phase-0 nudge; geometry flags are TODO.
