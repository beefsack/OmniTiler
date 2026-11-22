# KDE protocol wire cut-over: live smoke

Built and deployed the broker and KWin script together from `dfb70175` with
`OMNITILER_TRACE=1 just dev-on`. Ran on the real Plasma session. The fixtures
below joined one-to-one by sentinel title and PID to native UUIDs before
comparing KWin state. KGlobalAccel `invokeShortcut` exercised callbacks, not
physical keys. The scoped legs used disposable settings and intent roots;
no stored tiler settings were changed.

| Commit | Leg | Verdict and limit |
| --- | --- | --- |
| `223454ff` | Unscoped numbered select and send/follow on the shared per-output roster | **Pass (R)** for exact view and member receipts. Select moved only the current desktop. Send moved A from D2 to D1, left B in D2, and selected D1 with A active. The next Describe retired the transfer and unlocked A. The separate focus-bearing switch terminal did not publish; see below. Mismatched or partial receipts were not injected. |
| `da69c63b` | KWin `DiscoverWorld`: malformed settings, then stale-generation settings | **Pass (S)** for five-key codec and generation errors. Natural handshake/settings also succeeded. The stale probe used a separate KWin callback; it did not drive the controller's in-flight settings rediscovery branch. |
| `fa60a5f3` | Ordinary move and renderer snapshot | **Pass (F, P)** for strict full-reply interoperability and the missing `effects` key in the raw projection. No safe production stimulus for out-of-shape settings keys, more than 16 outputs, or retired offline v3 vectors. |
| `d253fe52` | Maximize A, then move it directionally | **Pass (M).** Native maximize mode changed from full to restored and the re-tiled frame matched the projection. The follow-up Describe carried `continued-tiled`. |
| `92505653` | Move A through ordinary frame layout | **Pass (F).** Frame setter and readback matched the joined native frame; the next strict Describe accepted the layout/called evidence. Other tag arms were not exercised. |
| `b90837e0` | Normal frame/state captures as controls | **Not live-exercisable (F, M)** for changed multi-defect refusal precedence. Injecting malformed evidence at strict decode would advance the active producer's capture sequence floor. |
| `fc6e3935` | Ordinary frame feedback into the next capture | **Pass (F)** for the required identity on the strict live path. The trace prints the non-null stamp but not the ID value. Clockless replay remains covered offline. |
| `cf71a7be` | Ordinary stamped frame/state captures as controls | **Not live-exercisable (F, M)** for holding future-stamped evidence while accepting valid siblings; that injection would compete with the active producer and could publish synthetic facts. |
| `ad9ae896` | Normal KDE captures plus wrong outer version and nested version refusals | **Pass (C, F).** An ordinary facts-only capture applied. The wrong outer version refused without binding; nested `v` under a valid outer version refused with the source and watermark. |
| `1fa340eb` | `ReadWorldPresentation` during the move | **Pass (P).** The exact narrowed key sets matched and the scenario reader's allocations matched both native fixture frames. The KDE subscriber also received a correlated presentation reply. No claim about painted pixels. |

## Decisive evidence

All excerpts are from the `dfb70175` deployment. Bracketed terms redact
session-specific identities, native IDs and geometry. Sources: `journalctl
--user` filtered to the KWin D-Bus-owner PID, the Planner stderr from `just
dev-on`, and `busctl --user`/fixture reads through the KDE live/scenario helpers.

```text
R (journal, invokeShortcut omnitiler-workspace-2, unscoped):
  omnitiler:world:applied rev=5 geom=0 member=0 views=1 mk=0 rm=0 fx=0 held=0
  omnitiler:world:trace ev=settlement-switch phase=receipt gen=[gen] req=[roster-proof] rev=6 obs=6 out=[output] op=op-1 outcome=complete focus=unproven fallback=false failed=-
R (journal + Planner, invokeShortcut omnitiler-move-workspace-1, unscoped):
  omnitiler:world:applied rev=4 geom=0 member=1 views=1 mk=0 rm=0 fx=0 held=0
  ev=observe-transfer snap=reply src=[send] rev=4 op=op-1 kind=window win=[A] out=[output] ws=ws-1 held=1 follow=1 cmdSeq=1
  ev=observe-applied snap=fresh src=[send-proof] rev=5 … replyXfers=0
  ev=observe-plan snap=reply src=[send-proof] rev=5 win=[A] out=[output] ws=ws-1 member=controlled/- frame=controlled/-
  ev=focus-eval … req=[send] rev=4 … decision=deferred reason=not-ready
  ev=focus-eval … req=[send-proof] obs=5 rev=5 … decision=equality reason=already-target
  ev=settlement-receipt … req=[next] rev=6 obs=6 ops=0 switches=0 gesture=none
S (KWin callDBus DiscoverWorld):
  {"v":4,"outcome":"rejected","kind":"malformed","message":"request is malformed","gen":"[gen]"}
  {"v":4,"outcome":"rejected","kind":"generation-mismatch","message":"generation-mismatch","gen":"[gen]"}
F (journal + Planner, directional callback followed by observe):
  omnitiler:world:trace ev=frame-attempt applySrc=[move] win=[A] target=[tile] setter=accepted readback=[tile] pre=- op=- attemptMs=282191 dpi=-/-
  omnitiler:world-summary direction=egress req=[frame-feedback] outcome=accepted kind=- rev=13 msg=-
M (journal, maximize and directional callback):
  omnitiler:world:trace ev=state-attempt applySrc=[clear] win=[A] property=maximized target=0 rev=4 setter=accepted pre=1 post=0 attemptMs=394
  omnitiler:world:trace ev=settlement-operation phase=receipt gen=[gen] req=[clear-proof] rev=5 obs=5 win=[A] kind=clear op=op-1 outcome=continued-tiled cause=-
C (KWin callDBus DescribeWorld; reply excerpts):
  "kind":"unsupported-version" … "nativeInvalid":"0","outcome":"rejected","source":"","v":4
  "kind":"unknown-field" … "nativeInvalid":"3","outcome":"rejected","source":"smoke-nested-version","v":4
P (busctl ReadWorldPresentation; reply key sets, and Planner subscriber receipt):
  coveredInstance coveredInvalid gen obs outcome presentation preview req rev v windows
  presentation: border foreground underlay
  omnitiler:world-summary direction=egress req=[subscriber] outcome=presentation kind=- rev=12 msg=-
```

The fixture-scoped send/follow and view-only select both stopped with
unchanged native state. The scoped view reply had `kind=view … held=1` and
`frameAuth=hold frameHold=unreadable memberAuth=hold memberHold=unreadable`.
This is an expected hold with scoped-out occupants in the source slot, not a
readback rejection. `kwin/src/world-observe.ts:916–921` excludes foreign
window rows but `:1489–1553` counts native membership. With both fixtures in
D1, the corresponding unscoped trace showed five observed windows there.
`crates/tiler-core/src/world_roster.rs:847–869,942–973` requires reconciled
occupancy and a writable source; `crates/tiler-core/src/world_publish_v4.rs:
52,145–186,236–241` masks the unproved transfer and publishes the hold.
[contract](../porting/world-contract.md#shared-native-roster-slot-plans)
requires residual occupancy to hold. No fence was relaxed.

### Open finding: no terminal for a successful send/follow

The controlled destination plan and removal of the held transfer on rev 5
prove the exact member readback reached the roster comparator; rev 6 still
carried no switch terminal. After KWin deferred focus for readiness, the
core retired global focus on foreground equality
(`crates/tiler-core/src/world_close_focus.rs:519–535`). Selection completion
independently requires the exact focus feedback with arrival brackets
(`crates/tiler-core/src/world_selection.rs:1634–1649,2297–2307`).

No user-visible failure was observed: the window moved, the destination
became current, and A was active. What is missing is the `switchSettled`
wire receipt. KWin's reader traces that receipt rather than painting from it.
This leg did not inspect rendered borders or focus history, or issue a later
command to test for knock-on effects.

`git blame` puts KWin's readiness deferral in `24b9d451`/`90391310`,
the equality retirement in `090a72cd`, and the independent arrival check
in `a4c78876` with shared-roster matching in `90391310`. All predate
`223454ff`. Diffing those paths through `1fa340eb` shows no change to
that interaction: `92505653` retags feedback, but the early equality
branch runs before reading it. `223454ff` changes roster proof, not
follow-focus readiness or completion. This run did not deploy the
pre-range version, so the provenance is from code, not a live bisect.

Options: leave this as a diagnostic gap, or fix the receipt path. Recommend
the latter: preserve or request an exact, readiness-checked arrival receipt
after equality with no authority to activate again. Do not accept bare
foreground equality as arrival or report success without the receipt.

Dev logs were copied before `just dev-off`. Owned helpers unloaded and
fixtures exited; no input daemon was started. Final `just dev-status`:

```text
dev mode: DOWN (planner unowned, controller unloaded)
```
