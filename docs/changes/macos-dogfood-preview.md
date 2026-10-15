# macOS basic tiling dogfood preview

## Goal and acceptance

Extend the [scoped tile preview](macos-scoped-tile.md) into a foreground macOS
`just dev [verbose|trace]` preview over ordinary applications on each display's
current native Space. Basic tiling comes first; managed workspaces and effects
remain later work. Stop leaves native frames in place and releases input owners.

- Select the event tap tentatively as the primary product input route; retain
  Carbon only for a concrete probe purpose.
- Recover and log native timeout/user-input tap-disable notifications. Prove a
  real OS timeout notification and continued exact-chord suppression with a
  bounded probe-only callback stall.
- Expose Authentic/Compatible presets through the existing catalogue model.
  Authentic consumes supported exact chords while running; Compatible leaves
  known conflicting chords to macOS. No persistent native-setting writes.
- Add unscoped application selection, own-bundle and configurable bundle-ID
  exclusions, configurable Meta, and supported basic shared-catalogue actions.
- Provide foreground log streaming, log paths at start/end, graceful Ctrl+C,
  bounded teardown, and a short dogfood guide.
- Pass macOS tests, strict all-target clippy, format, signed-bundle verification
  and hosted CI. Live evidence uses scoped disposable targets; unscoped
  selection is verified offline before user dogfooding.

## Approach and bounded units

1. Input route, preset/catalogue reuse, disable recovery and bounded native
   timeout probe; preserve the existing eligibility and synthetic-target fences.
2. Unscoped selection/configuration and the macOS foreground dev launcher,
   retaining the bounded scoped tile command for focused tests.
3. Independent review of live/input/lifecycle boundaries, scoped verification,
   final gates and publication.

## Decisions and evidence

Initial scope follows the 2026-10-15 product decisions. Previous scoped evidence
remains in its original record; it does not establish unscoped acceptance.

- The product now selects the tap by default; Carbon remains only in diagnostic
  probes. The platform-local catalogue mirrors the existing Windows/KDE basic
  actions and invokes shared core commands: focus/move, float, directional
  grow/shrink, and orientation (34 exact chords). There is no existing portable
  shortcut catalogue to reuse. Managed-workspace and native-state actions stay
  unbound.
- Compatible leaves only the established unshifted Ctrl+arrow conflicts to
  macOS (30 chords); shifted moves and Alt resize arms have no established
  Mission Control holder. Authentic retains all 34. No native preferences change.
- Settings are read at startup from the per-user application-support
  `OmniTiler/settings.json`; CLI fields override the file. Unscoped exclusions
  do not affect explicit scoped runs. Own bundles are excluded unscoped;
  terminal applications retain ordinary product classification.
- Review found that process termination did not prove graceful teardown. The
  foreground launcher now requests stop through an exact-run directory and
  waits for the owner's bounded runtime shutdown result. Atomic launch ownership,
  per-run nonce/start identity, contained control paths and retained ownership on
  unknown exit prevent overlapping owners. An independent stop-file watcher
  starts before unbounded runtime startup and bounds stop even if the controller
  stops pumping. Missing/dirty outcomes retain evidence rather than claim clean.
- Review also found a real silent-disabled condition: a stalled tap could read
  disabled without either native disable notification. The input owner now
  checks enabled state on its existing pump turns, logs this as a separate
  unnotified-disable kind, and verifies re-enable or recreates the same chords.
  Timeout and user-input notifications recover without treating the latter's
  enum name as proof of revoked consent. Lock/Secure Input/eligibility gates
  still precede interception. User-input notification/recovery is live-proven;
  enabled-readback-only recovery retains offline coverage, not a live claim.

### Scoped native evidence — 2026-10-16

- Plain tap/HID pipeline: exact-pair suppression observed in 48 ms; unmatched
  extra-modifier/plain pairs delivered +1/+1; Ctrl+Left/Right suppression in
  36/39 ms with current Space and layout unchanged. Helper focus/restore
  agreement observed in 134/113 ms; helper exit and absent CG residue verified.
  This predates the expanded catalogue and recovery correction.
- Actual timeout notification remains **unproven**. A changed HID-posted stall
  hypothesis reached the callback for 1.2 s but returned zero timeout/user-input
  notifications. Direct enabled readback later returned false. This motivated
  the separately classified health-recovery correction, not a timeout pass.
  Together with the earlier session-posted variants, synthetic approaches are
  exhausted; the remaining discriminator requires bounded physical HID pressure.
- Both presets completed scoped arming-only runs with clean shutdown. No
  in-scope application was present: this is not preset-delivery or tiling proof.
- Scoped legs restored original foreground and Spaces; disposable helpers,
  taps and display-sleep-prevention children were cleaned.

### Final scoped journeys

All key-post evidence is synthetic native-pipeline evidence. Physical keyboard,
visual shell, games, multiple displays and minimum-supported-OS acceptance are
separate. Final tests used two disposable signed helper windows and the explicit
helper-bundle scope; no unscoped desktop run was used as evidence.

| Journey | Observed result |
| --- | --- |
| Admission / focus / move | Two distinct half-work-area frames, AX=CG; bidirectional focus readbacks agree; move swaps and follows the mover. Input-to-applied spans 15–42 ms in the final float/resize run. |
| Float / unfloat | Fixed an observation feedback defect: AX has no native intentional-float flag, so the adapter now echoes retained session placements without confusing placement-less fullscreen holds. Sibling expands to full width, floated frame stays put and remains stable for over 12 s; unfloat restores halves. |
| Grow / shrink | Focus-first, interior shared-boundary grow changes both shares by 12 points; shrink restores the halves. Earlier exterior-edge no-ops were correct refusals, not resize defects. Offline tests also pin the shared accelerated step schedule. |
| Orientation | Horizontal → vertical → horizontal, AX=CG; input-to-applied 40–67 ms. Held-repeat suppression is offline-tested; the live producer posts discrete pairs. |
| Compatible | 30 armed chords. Unshifted Ctrl+Left/Right deliver two downstream pairs without Engine input or layout changes; shifted move-left still swaps. Current Space/layout unchanged. |
| Native UserInput recovery | Probe-only `--disable-once` produced a real `TapDisabledByUserInput`: user=1, timeout=0, unnotified=0. Same-tap re-enable read back enabled; subsequent exact pair suppressed in 49 ms, unmatched extra/plain pairs delivered +1/+1. Arrow suppression 40/41 ms; after exit formerly owned pair delivered +1/+1. |
| Foreground lifecycle | Scoped no-target `just dev trace`: natural completion clean; second owner refused; `dev-stop` ended the origin stream with clean shutdown; Ctrl+C produced `stopped`, clean teardown and exit 130, with paths at both ends. Owner process absence and cleared ownership verified. |

The float correction initially over-retained placement-less fullscreen holds;
classification regressions caught it and the placement-qualified correction
passes the full suite. A probe-only Carbon-status fixture also omitted Alt when
probing expanded arrow arms; corrected full-modifier registration produced all
16 distinct arrow registrations without duplicates. Neither is a Carbon
physical-delivery acceptance claim.

Final scoped readbacks restored the original foreground through the selected
oracle and retained original Space layout/current. Helpers, taps, wrappers and
display-sleep-prevention children were reaped; test-control records were cleared.

## Dogfood guide

From the repository root, with the documented persistent development signing
identity and Accessibility grant already set up:

```sh
mise exec -- just dev
mise exec -- just dev verbose
mise exec -- just dev trace
```

Each invocation builds/signs the current bundle before LaunchServices launch.
The default signing identity is `OmniTiler Mac Dev`; `OMNITILER_DEV_IDENTITY`
can select another configured persistent identity. The terminal prints retained
stdout/stderr log paths at start and end. A second dev owner refuses.

Bare `dev` tiles ordinary apps, including terminals, on the current user Space
of each display. Press **Ctrl+C** once for graceful stop, or from another terminal:

```sh
mise exec -- just dev-stop
```

Expect a verified clean shutdown message. Ctrl+C can return 130 through the
wrapper/just interruption path; the clean owner result establishes teardown.
Stop leaves window frames in place, releases input, and makes no native shortcut
preference changes. Logs remain available at the printed paths.

Default shortcuts (Ctrl is Meta):

| Chord | Action |
| --- | --- |
| Ctrl+H/J/K/L or arrows | Focus left/down/up/right |
| Ctrl+Shift+H/J/K/L or arrows | Move left/down/up/right |
| Ctrl+Alt+H/J/K/L or arrows | Grow toward that boundary |
| Ctrl+Shift+Alt+H/J/K/L or arrows | Shrink from that boundary |
| Ctrl+G | Toggle intentional float |
| Ctrl+O | Toggle split orientation |

Authentic is default. Select Compatible or a different Meta at startup:

```sh
mise exec -- just dev --preset compatible
mise exec -- just dev --ctrl meta
mise exec -- just dev --exclude com.example.App
```

`meta` means Command. Alt/Shift cannot be the sole Meta for this full catalogue
because those modifier arms would collide. Optional startup settings live at
`~/Library/Application Support/OmniTiler/settings.json`:

```json
{
  "modifier": "ctrl",
  "preset": "authentic",
  "excluded_bundles": ["com.example.App"]
}
```

Missing settings use defaults; malformed/unknown fields refuse with a reason.
Explicit CLI fields override the file. Exclusions are exact bundle IDs and apply
only unscoped. For a bounded application-only preview:

```sh
mise exec -- just dev trace --scope com.apple.TextEdit --seconds 30
```

Known gaps: no managed workspaces, borders/underlay or other effects; native
fullscreen remains untouched; no maximize/sticky/native-fullscreen bindings;
physical Carbon delivery is unverified and Carbon is diagnostic-only. Actual
OS timeout notification recovery still needs physical-input evidence. Fixed-size
applications can clamp tiled slots; the earlier sheet/classification and
multi-display/game discriminators remain open. This is a basic tiling preview,
not feature-complete macOS delivery.

## Verification and publication

- Final macOS tests: **445 passed**, zero failed/ignored; portable core,
  protocol and effect-FFI tests pass. Strict all-target clippy with `-D warnings`,
  workspace format, just format, shell syntax and whitespace checks pass.
- Foreground shell harness: **94 checks pass**, including startup/interruption,
  malformed/dirty outcomes, concurrent runs, stale/reused identities, contained
  control paths, log streaming and exact ownership clearing.
- Final signed-bundle build and strict verification pass for both bundles,
  including macOS 15 deployment load commands and the persistent certificate
  requirement. Probe SHA-256:
  `094c5431bde6b5c36b57aa273f9c06f19eebd524287a00940a40a7de1af8e972`;
  helper SHA-256:
  `40c4275782b29b749f191f6afb6a849ac4a2ad808da63e510d2f3da7c21044fb`.
  Identifiers remain `io.github.omnitiler.macos.probe` and
  `io.github.omnitiler.macos.probe.helper`, with certificate leaf
  `040f96377aaf110a77905d453bb76681e7133319`. Development Gatekeeper assessment
  remains diagnostic; no security-policy changes were made.
- Hosted CI is pending publication.

## Remaining

Publication gates are pending. Actual OS timeout-notification recovery needs a
bounded physical-input discriminator; the synthetic stall modes must not be
reported as a timeout pass. Physical Carbon delivery remains unproven and is no
longer a product-route prerequisite. The next product acceptance journey is
unscoped user dogfooding with `mise exec -- just dev trace`. This record remains
active until the outstanding native timeout evidence is resolved.
