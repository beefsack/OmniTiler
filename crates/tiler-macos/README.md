# tiler-macos — Phase 0 capability probe

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

Threading: main thread AppKit/event dispatch only; dedicated input thread for
the event tap with a minimal callback; Carbon hotkey wait on the main thread
(Carbon dispatches there) in bounded runloop slices; per-app AX threads with
finite messaging timeouts and bounded runloops; engine thread owns state via
a bounded `sync_channel` (`try_send` backpressure, never a block in a
callback).

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
PID with exactly the recorded nonce title). No process-identity machinery
and no `NSRunningApplication.launchDate` token anywhere. Live status:
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
`meta`; fixed probe chord `0x04`):

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
live unproven: whether full agreement holds is measured, never assumed.

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
(UNTESTED live): it authenticates the full spawn record
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
full validation (usable after the mover exits, via the spawn record), so the
independent restore path is proven on its own — but independent restore is
NOT considered proven until a live test demonstrates it.
Source and destination must read back as user Spaces; unreadable Space type,
unknown frontmost context and loginwindow fail closed before dispatch. A
frontmost observation is not an atomic lock detector. Source and target must
map unambiguously to the same display; cross-display recovery remains unproven.
`spaces-switch`, `spaces-create` and `spaces-destroy` stay explicitly
unsupported: their non-SA routes are unimplemented here with recovery unproven
(see the [route survey and live plan](../../docs/research/macos-port/native-spaces-routes.md)) and the commands report the route instead of
executing.

Native-Space write routes (source-only determination at pinned yabai
`dd845723416f5fe92af49fad5ebab00369e07edd`; live effects UNTESTED; no SIP
rejection asserted without live evidence):

| Operation | Tier-2 route | Scripting-addition only? |
| --- | --- | --- |
| Window move | Direct `SLSMoveWindowsToManagedSpace` only (UNTESTED, possibly ineffective on the macOS 15 floor); compat-ID fallback disabled (no exact restoration); bridged async op detected, never executed ([space_manager.c](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705), [extern.h](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L63-L63)) | No (SA attempt omitted) |
| Switch vs focus | Yabai non-SA gesture fallback; AX Mission Control and enabled system chords are additional candidates | No; unimplemented here, recovery unproven |
| Create / destroy | Yabai commands use SA; Hammerspoon uses non-SA AX Mission Control | Yabai yes; UI alternative no, unimplemented here |
| Reorder / Space-to-display move | Yabai SA internal Dock functions | Yes in surveyed yabai; no proven non-SA implementation |

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
- Owned Space move/restore: direct-only implementation, UNTESTED live
  (source shape only, no live dispatch or readback evidence yet; independent
  restore unproven until a live test; `--allow-untested` required per
  invocation). Switch/create/destroy: explicitly unsupported (no safe native
  recovery); the commands report the pinned route instead of executing.
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
