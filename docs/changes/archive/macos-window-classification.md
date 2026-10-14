# macOS normalized window policy and classification

## Goal and scope

Extract a small pure, platform-agnostic window-policy API into `tiler-core`
and consume it through the offline macOS observation skeleton. Pin existing
Windows policy and equivalent KDE outcomes without changing either adapter.
Normalize AX/CG facts locally; keep native observation, actuation, lifetime
proof and anti-fighting platform-owned.

## Acceptance and approach

- Additive core modules cover overlay refusal precedence, born-fullscreen
  hold, admission-clear eligibility and the admission/origin distinctions
  needed by macOS. Reuse shared fixed-size and born-maximized policies.
- Conformance vectors compare actual existing adapter predicates where
  possible; document any unmatched/native-capability lanes explicitly.
- macOS fake observations prove role exceptions, shared fixed-size admission,
  fullscreen hold/exit, minimize/hidden layout exclusion and process policy.
- Read pinned local reference sources for yabai, AeroSpace, Amethyst, rift and
  glide. Record source-policy divergence with upstream SHA links. Straightforward
  macOS choices are tentative; disputed outcomes remain discriminating TBD rows.
- Extract K2 metadata only where the skeleton actually consumes it; preserve
  target-specific K3 logic and existing core behavior.
- Verify macOS-capable workspace tests, macOS tests, all-target strict clippy,
  formatting and hosted Linux/Windows/macOS/native-effect CI.

## Bounded work

1. Shared policy and conformance, based on current Windows/KDE predicates and
   normative window-state/fixed-size decisions.
2. macOS reference comparison, pure classifier and fake engine integration.
3. Diff/contract review, integration verification and concise decision/audit
   records.

## Implementation and evidence

- Added `tiler_core::window_policy`: overlay refusal priority, born-fullscreen
  hold, safety/geometry/focus eligibility, admission-clear and reserved-slot
  gates, and ordinary-versus-sticky B9 unfloat eligibility. Existing fixed-size
  and born-maximized policies are reused; existing Engine behavior is untouched.
- `test-fixtures/window-policy.json` has 39 rows. Ten portable tests call the
  actual Windows policy copy and core functions over the same inputs, including
  all 512 safety combinations. The macOS test-only Windows dependency adds no
  external dependency. Private native admission-clear code is not invoked:
  its identical boolean formula is compared through public geometry/focus
  classifiers. KDE equivalence is source-reviewed, including its different
  orientation refusal token; no adapter rewiring is included.
- The macOS classifier carries independent fullscreen and float-origin facts,
  hints and AX size-settability; recognized dialogs/sheets/panels/modal windows
  are untouched floats, including nonzero-layer panels. Standard windows reach
  shared admission. Own-bundle, minimized/hidden and conservative policy/kind
  exclusions are distinguished. Offscreen alone is not destruction. Ghostty
  is ordinary. Native readers still do not supply these facts.
- Cold fullscreen-only, fixed-only and mixed first observations initially
  refused. Adapter wire normalization now supplies existing synthetic-hold and
  automatic-fixed provenance, using read-only Session queries. Cold admission,
  D5 current-hint first exit, two-tile reserved fullscreen allocation, D2 hint
  changes, transient overlays and minimize/hidden recovery fixtures pass.
- No new K2 catalog/schema was needed: the skeleton consumes existing core
  commands and the existing fixed-size setting enum/default. K3 remains local.

## Reference comparison

Source-policy inspection only, at the [recorded checkout pins](../../research/macos-port/prior-art.md#source-checkout-snapshot-2026-10-14);
none establishes native acceptance.

| Reference | Evidence / divergence |
| --- | --- |
| [yabai](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1495-L1510) | Hidden/minimized/fullscreen stay untiled; nonstandard, non-normal-level and small non-resizable windows float. Dialog subroles can count as standard; OmniTiler explicitly floats them. No small-frame heuristic copied. |
| [AeroSpace](https://github.com/nikitabobko/AeroSpace/blob/74a1bf17e82d70e0a21945ba04bb4f590bf19f83/Sources/AppBundle/model/AxUiElementWindowType.swift#L35-L91) | Nonstandard subroles and button heuristics select dialogs, with app exceptions. No button/app-specific heuristic copied, including its Ghostty rule. |
| [Amethyst](https://github.com/ianyh/Amethyst/blob/5cfe2fe12b1a933139010da5760aaa7e44033e9b/Amethyst/Model/Window.swift#L269-L295) | Requires movable standard subrole; optional small-window floating. OmniTiler retains recognized transients as floats instead of dropping them. |
| [rift](https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/actor/reactor/utils.rs#L48-L88) | Root/window/standard and layer gates, with an IBKR dialog exception. OmniTiler has no application-specific exception and retains recognized layered floats. |
| [glide](https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/layout.rs#L333-L351) | Nonstandard and non-resizable float by default. Shared OmniTiler fixed admission remains hint-based; AX size-settability alone does not become invented equal bounds. |

## Verification and outcome

- Post-rebase macOS-capable locked Rust gates: 1,896 tests passed, no failures or
  ignored tests, including 231 macOS tests. All-target strict clippy for core,
  protocol, effect FFI, Windows and macOS passed; workspace fmt and diff checks
  passed. `omnitiler` is the Linux KDE planner/tray, not an expected macOS
  build target; Windows/macOS CI explicitly tests the portable/native-platform
  crate allowlist instead. Trying full workspace tests on macOS reaches its
  existing `float_intent_store.rs` `stat.st_mode`/`S_IFMT` u16/u32 mismatch.
  No KDE code is changed. Full Linux workspace and native-effect gates passed
  in hosted CI.
- Review found that a minimized tiled member restored directly fullscreen loses
  its D23 slot in existing core convergence. An investigated macOS wire-omission
  workaround preserved the slot by withholding the known live window; that
  divergence is removed. **Tentative A, pending later user review:** truthful
  reporting preserves adapter/core parity; desired shared slot retention stays
  TBD, with a tentative P2 candidate for a later separate shared-core change.
  An asymmetric three-window regression pins current fresh admission on exit
  without weakening the ordinary D23 restore oracle or allowing fullscreen writes.
- [Minimal discriminators](../../spec/reference-outcomes.md#macos-classification-discriminators)
  retain this question plus settable-only admission, activation/layer exceptions,
  nonstandard application windows and sheet identity as TBD. No native outcome
  is inferred from passing fake tests.
- Implementation `7da6d1e` passed [hosted CI](https://github.com/beefsack/OmniTiler/actions/runs/38610743082):
  all eight jobs succeeded (Linux workspace Rust, KWin/typecheck, shell,
  Windows, macOS, native effect/settings, licenses and notices).
- A subsequent [documentation-head run](https://github.com/beefsack/OmniTiler/actions/runs/38612022741)
  exposed an existing structured-log ordering race: sequence allocation and
  ring insertion were locked, but enqueue after unlock could deliver a later
  sequence first. A concurrent public-observation probe reproduced eight
  inversions before correction and passed three times afterward. Sequence/ring
  insertion and bounded `try_send` now share the existing lock; sink I/O remains
  logger-only, with unchanged bounded/drop semantics. The existing monotonic
  sink assertion is preserved; all 231 macOS tests, strict clippy and fmt pass
  after the correction. Corrective publication CI is attached to its commit.
- Offline implementation and cross-platform verification are complete.
  Native AX acquisition, real Spaces routing and live capability acceptance
  remain later Phase 1 work. Classification decisions remain tentative as
  recorded; unsupported outcomes stay TBD.
