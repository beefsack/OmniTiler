# macOS native workspace capability verdict

## Goal and acceptance

Establish native Space window-move and selection routes with SIP enabled before
integrating workspaces into the macOS dogfood preview. Native Spaces remain the
selected model; scripting additions, Dock injection and parking are excluded.

- Create exactly one disposable user Space; identify it by the ordered
  per-display snapshot difference.
- Compare eligible non-SA move/back and switch/back routes with membership,
  current-Space and focus readbacks, bounded latency and honest jank evidence.
- Prove independent owned-window restoration before API moves. Remove only the
  created, inactive Space and verify the original ordered topology/current state.
- Distinguish unavailable, unimplemented, consent-blocked, observed no effect and
  explicit rejection. None alone establishes a SIP cause.
- If move and selection are viable, deliver native per-display workspace mapping,
  independent layouts and shared workspace actions with fake-driven coverage and
  scoped native verification. Otherwise retain the capability verdict and exact
  outstanding prerequisite.

## Approach

1. Inspect the existing probe boundary and pinned local upstream implementations.
2. Implement and review bounded Mission Control AX lifecycle/readback support,
   then establish the temporary-Space fixture and independent recovery.
3. Compare eligible window-move and selection routes; update the
   [route survey](../../research/macos-port/native-spaces-routes.md) and
   [Phase 0 evidence](../macos-phase-0.md).
4. Gate preview integration on accepted native effects and restoration; verify
   mapping/actions offline and use only disposable application scopes live.

## Outcome — 2026-10-16

The capability comparison is complete for eligible same-display routes. **Native
selection and create/remove are viable; ordinary-window transfer remains
unresolved. No workspace MVP was enabled.** This is a blocked capability verdict,
not proof of universal SIP-on impossibility. The preview remains current-Space
tiling; native Spaces and tier 2 remain selected.

Canonical [per-route evidence and continuation](../../research/macos-port/native-spaces-routes.md#live-verdict--2026-10-16):
MC AX create 448 ms, settled selection approximately 628–660 ms, remove 515 ms;
direct SLS dispatched once with no effect through delayed readback; bridge
dispatcher absent/class present; compat exact-tag restoration blocked;
digit/drag bindings unresolved; enabled arrow candidate no effect; same-display
Dock-swipe both ways observed. Jank unobserved; gesture focus/post measurements
are not end-to-end switch latency. Minimum-supported-release, multi-display and
ordinary-app acceptance remain separate.

Delivered bounded probes: MC inspection/lifecycle/recovery/member diagnostics,
effective chord corroboration without symbolic-ID invention, public owned-helper
recovery with exact collection-behavior restoration, direct move comparison after
independent restoration, and guarded arrows/gesture comparisons. Fake-driven
coverage exercises mapping joins, removal identity/emptiness, asynchronous
readback, dispatch uncertainty, holder ownership, pending input release and
cleanup. No shared core behavior or native preference was changed.

### Material corrections and accepted recovery

- SDK-pinned AX error decoding corrected omitted constants; `NoValue` no longer
  masquerades as another error.
- Initial integrated lifecycle failed by checking an asynchronous switch too
  early. Poll-only readback repair proved return; its integrated full rerun is
  still pending. The separately corrected create/select/remove legs passed.
- The first public-recovery fixture failed before self-action because it used
  cross-command title authentication on an off-current window. The documented
  held-Child/pipe-nonce/executable/CG proof corrected that fixture; O→T→O then
  passed before the private move. Standalone title checks were preserved.
- Removal parked before dispatch on compositor chrome, then on a versioned
  executable alias. Exact system-file canonicalization and narrowly observed
  shell exclusions resolved those prerequisites; a single remove then passed.
- Synthetic switch comparison reported focus residue. Exact-target public app
  activation restored foreground without keys/events/window setters. An internal
  offscreen non-Space chrome self-resize was reported rather than written back;
  ordinary protected frames/memberships remained identical.

Final restoration: sole temporary Space absent; ordered per-display SIDs/current,
topology, separate-Spaces state and shortcut overrides matched B0; owned helpers,
probes/taps and sleep-prevention children were gone/reaped; Ghostty frontmost by
the full selected oracle.

## Verification and publication

- Final macOS/portable allowlist: **1,443 tests pass**, zero failures; 425 macOS
  library tests. Strict all-target clippy with `-D warnings`, workspace format,
  shell syntax and whitespace checks pass. Later evidence-wording edits pass all
  425 macOS library tests again; no behavior changed.
- Persistent bundle identity `io.github.omnitiler.macos.probe`; certificate leaf
  `040f96377aaf110a77905d453bb76681e7133319`. Strict signed-bundle verification
  passed. Final **live cleanup** artifact SHA-256 (preceding the later mechanical
  observation-module split and evidence wording):
  - Main: `da9cfb6f811510dc4d5803295235d31e20a94f08c2c5b58a43170d961f7c7795`.
  - Nested helper: `c96452604826f14e6fde1132c4971998123abdd332b357a195216dd32fbcc317`.
  - Helper bundle: `839a724a00de1f03a11e7b972097f4334944f1aa671a867e0eb36293c977a588`.
- Final-source signed rebuild and strict verification also passed after the
  mechanical split and fixture-data cleanup (no additional live effects).
  Main SHA-256: `912034319b50208ff9f89f1264539a263727d44fd9fd5925b39a1ac2ca4bbd09`;
  helper-bundle SHA-256:
  `218ee4ac6866479a3051fc1be3f70651ea258830ede8241eba9d29abc13d022a`.
- Source publication: `c3dba6c`. Initial hosted CI passed macOS and all non-Rust
  jobs, but Linux/Windows strict clippy caught macOS-only imports/helpers in
  portable stubs. Scoped cfg/lint corrections preserve refusal behavior; the
  portable/macOS allowlist tests, strict clippy and formatting pass afterward.
  Final hosted publication check pending.

## Parked and exact next action

Establish a safe ordinary-window transfer/inverse by an authoritative compat-tag
preimage or an actually available bridged dispatcher; do not retry the direct
no-effect experiment unchanged. Drag fallback needs resolved already-enabled
Desktop-N chords and recoverable input. Product numbered/relative select,
send/follow/stay, per-display layouts, trailing-empty creation and background
frame writes remain pending that gate. Visual animation/jank requires actual
observation. No parking substitute or SIP-policy change is selected.
