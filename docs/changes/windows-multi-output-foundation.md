# Windows Multi-Output Foundation

## Goal And Scope

- Enable exhausted directional tiled movement, horizontal directional focus,
  and native/Win-drag drops into another output's current layout.
- Reuse shared Engine topology, local-first movement, R4 landing and pointer
  admission semantics. Use full output rectangles for adjacency and each
  output's work area and DPI-aware geometry for placement.
- Establish complete current-view observations and lifetime, membership,
  visibility, geometry, recovery and focus fences before native transfer.
- Explicit output-send bindings, non-local workspace modes, whole-workspace
  migration and migration source-refill remain separate handoff work. Window-
  based output selection is reused where required by directional movement.
- Explicit output follow/stay subsequently delivered offline `fda618f`,
  2026-10-14 ([record](archive/windows-explicit-output-send.md)); position-based
  selection audited and integrated for current window paths
  ([record](archive/windows-position-based-output-selection.md)). Foundation
  physical journeys below remain pending; migration stays item 14.
- No hotplug acceptance or ordinary-floating transfer claim.

## Acceptance

- Local movement wins first; exhausted eligible tiles cross all four directions
  into the selected adjacent output's current workspace and follow focus.
- Exhausted horizontal focus selects the destination's valid last-focused tile
  without geometry or membership writes; empty/unreadable targets refuse.
- Native title-bar and Win-drag releases join destination tiling at the pointer
  drop point and remove source membership; preview remains render-thread owned.
- Synthetic two-output tests cover occupied/empty destinations, independent
  current views, differing work areas/scales, negative coordinates and other
  arrangements, stale lifetimes/topology/views and failed arrival/write fences.
- Locked four-package build/tests, formatting, strict all-target clippy and
  diff whitespace checks pass. Physical two-output journeys remain pending
  until separately observed.

## Diagnosis

- Both supplied sessions observe two outputs and reconcile managed windows in
  both domains. Missing monitor enumeration is not the cause.
- Repeated rightward moves first restructure locally, then return
  `planner-noop`; edge focus returns `unchanged`. Windows builds single-domain
  directional events with `cross_output_transfer: false`, so these are correct
  Engine results for the incomplete adapter, not a clamp or transfer refusal.
- Supplied gesture traces contain native resize operations, not a recorded
  move-drop. Their evidence cannot identify the reported title-bar drag's
  target. Independently, code restricts move drops to source-domain bounds and
  refuses cross-output releases.

## Approach

1. Complete adapter current-view pair construction and checked cross-output
   actuation; reuse core selectors and retained Engine transactions.
2. Route native and Win-drag target/preview/release through destination-current
   layout observation and the same transfer fences.
3. Review mutation boundaries and regressions, run native offline gates, and
   reconcile dated specification cells and durable decisions.

## Evidence And Remaining Work

- Draft triage (2026-10-13): build passed; tests reported 1,734 passed and one
  ignored, with formatting failures and unreviewed transfer publication.
- All three approach steps are complete offline. Physical two-output
  acceptance remains pending; this record stays active until that evidence.
- Shared additions include two-domain occupied drag transactions, retained
  per-domain focus and domain-scoped hover priors. Protocol conversion derives
  prior scope from existing DTO fields; no serialized field changes. Existing
  core/protocol/FFI suites and focused neutrality review pass.
- Churn audit: default diff alignment was misleading. Minimal diff reports
  6,184 additions and 80 deletions for the original native draft; downstream
  function order is unchanged. No accidental code motion required cleanup.
- Focused shared review confirms same-domain and existing singleton routes
  retain their outcomes, with unchanged DTOs and wire conversion. Existing
  sticky-hover, drag-refusal, capability and singleton protocol regressions pass.
- Keyboard and occupied/empty drag review found publication preceding final
  topology, readback and arrival checks. Corrected with candidate Engine
  planning and commit-last logical transfer; cross-focus MRU changes are also
  discarded on refusal. Failure regressions retain source membership and
  Engine placement and suppress follow-focus. Final independent boundary
  review confirms native lifetime/scope/veto checks remain intact and
  observation updates caches without publishing Engine or membership changes.
  Local-first focus now also retains candidate MRU only after verified focus.
- Tentative, pending user review: failed transfer retains original logical
  membership while observed partial native writes converge without replay;
  a verified transfer stands if its later focus actuation fails. See
  [decisions](../decisions.md#windows-multi-output-foundation).
- Synthetic coverage includes all-four adjacent-output selection and occupied
  Engine plans; occupied/empty keyboard and drag destinations; independent
  current views; negative/stacked arrangements and differing physical work
  areas; stale lifetime/scope/owner/view/topology/gap fences; late readback,
  arrival and empty-source-reflow refusal. Empty cross-focus refuses without
  writes or retained MRU changes. Caption-started modal moves remain moves
  when mixed-DPI frame rescaling would otherwise classify them as resize.
- `MonitorArea` carries physical full/work rectangles, not a scale field.
  Fixtures verify no adapter rescaling; real mixed-DPI settlement remains
  physical acceptance, not simulated native DPI evidence.
- Final gates (2026-10-13), rustc 1.99.0, MSVC target: locked four-package build,
  tests, workspace formatting, strict all-target clippy and diff whitespace
  checks pass. Tests: **1,748 passed, zero failed, one ignored**, +60 over
  baseline. Package totals: core 580, protocol 183, KWin FFI 40, Windows 945
  (580 library and 365 integration; one library test ignored).
- No physical or Linux/KWin-native acceptance is inferred from portable gates.
- Follow-up review (2026-10-13): `f2169d6` correctly initializes D23
  `closed_windows` in all three Windows cross-output test constructors and the
  shared Engine test helper. Production directional move/focus and both drag
  event paths inherit an empty set from the reconcile builder: transfer is not
  close, and absence alone must not clear restore slots. No defect found;
  explicit Windows close tracking remains handoff item 22. Current native
  four-package build/tests/fmt/strict clippy and whitespace gates pass:
  **1,790 passed, zero failed, one ignored**.

## Verification follow-up (2026-10-14)

- The scoped native lifecycle pass detected one output. A two-output journey
  could not be exercised; no directional transfer/focus or cross-output drag
  acceptance is inferred from same-output plan/readback events.
- Native locked four-package build/test, workspace format, strict all-target
  clippy and whitespace gates passed at `8c77d24`: **1,847 passed, zero failed,
  one ignored**. Scoped owner stop, settings/Mouse Snap restoration and clean
  ledger readbacks passed. Physical multi-output acceptance remains pending.

## Remaining Physical Check

1. Start the updated development build with tracing. Place ordinary tiled
   windows on both outputs; use different current workspace numbers.
2. Press Win+Shift+Right until local moves exhaust: the focused tile must
   arrive on the adjacent output, follow focus, fit that work area and leave
   source membership. Reverse with Win+Shift+Left. Repeat into an empty
   destination workspace; neither output's current workspace should switch.
3. Set a known last-focused destination tile. Exhaust Win+Right locally and
   verify it selects that tile without moving windows; reverse with Win+Left.
   An empty destination must leave focus unchanged; vertical focus stays local.
4. Drag a title bar across outputs and release at a destination tile edge;
   repeat using Win+left-button drag. Verify destination pointer placement,
   source reflow, focus and preview cleanup. Repeat into an empty destination
   workspace. Repeat mixed-DPI direction changes if applicable.
5. Stop through `mise exec -- just stop`; verify
   clean owner exit, restored visible windows and no overlay residue.
