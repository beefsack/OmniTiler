# Reference-rule comparison review

Scope: the 87 "comparisons requiring user review" entries from nine
source-fill notes, grouped by rule, tallied across all 12 reference
columns per semantic leg. Review analysis is the 3431275 baseline; recommendations
were decided by the user on 2026-10-10 below. Approved governance records and
the reference matrix remain authoritative; analysis is preserved.

Legend: C COSMIC, H Hyprland, B bspwm, I i3, X xmonad, S sway,
Q qtile, A awesome, N niri, P PaperWM, K karousel, R paneru.
Non-scrolling = C,H,B,I,X,S,Q,A. Votes: A agree, D differ,
U unsupported (no counterpart; never a vote), T TBD (applicable but
untraced; never a vote). Partial-TBD legs vote only the evidenced leg.
Scrolling N/P/K/R is reported separately from non-scrolling references.

Policy applied: a NORMATIVE requirement alone is not a recorded
deliberate deviation; only explicit intentional exclusion/deferral,
documented platform policy, or rationale addressing that reference
difference settles an entry. Uncovered COSMIC differences are MATERIAL
even as single outliers (no setting follows). A clear non-scrolling
majority difference is MATERIAL even when COSMIC agrees; settings only
for meaningful coherent alternatives, except single-outlier or scrolling-only
differences.
COSMIC is the default recommendation absent stronger documented user
intent. The analysis selects nothing; the dated user decisions below do.

Inputs: [cosmic](../changes/archive/cosmic-reference-source-fill.md),
[xmonad](../changes/archive/xmonad-reference-source-fill.md),
[awesome](../changes/archive/awesome-reference-source-fill.md),
[bspwm](../changes/archive/bspwm-reference-source-fill.md),
[hyprland](../changes/archive/hyprland-reference-source-fill.md),
[qtile](../changes/archive/qtile-reference-source-fill.md),
[niri](../changes/archive/niri-reference-source-fill.md),
[sway](../changes/archive/sway-reference-source-fill.md),
[i3](../changes/archive/i3-reference-source-fill.md) (8,11,11,8,8,10,11,9,11
= 87); scrolling notes + commits f04511c/c023391/b3d7265 hold zero
formal comparison entries; [matrix](../spec/reference-outcomes.md);
[functional-spec](../spec/functional-spec.md);
[decisions](../decisions.md); [consensus](./reference-wm-consensus.md).

## User decisions 2026-10-10

All eight MATERIAL groups and both RECORD DRIFT entries are decided;
the 35 NOT MATERIAL groups are unchanged. Current rules are integrated by
behaviour area in [decisions](../decisions.md) and the [spec](../spec/functional-spec.md);
shared-core/KDE changes are [delivered offline](../changes/archive/reference-comparison-implementation.md);
Windows wiring and native checks remain under [P1 delivery and Windows handoff](../backlog.md).

| ID | Selected outcome |
| --- | --- |
| G-01 | KEEP full-zero and unbounded-sentinel guards. Sentinel rejection is deliberate KDE host adaptation: KWin's marker means no limit, while ten references including COSMIC float raw-equal sentinels. D1 no longer claims plain COSMIC parity. |
| G-02 | KEEP relocated active workspace visible/focus retained on disconnect (deliberate COSMIC/6-reference deviation: focus continuity); KEEP nearest-surviving destination (deliberate COSMIC first-remaining deviation: spatial proximity, only discriminating with 3+ outputs). |
| G-03 | KEEP sole-window send-next filling trailing empty (deliberate COSMIC deviation: predictable send; COSMIC refuses, six references fill). |
| G-04 | KEEP migrated remembered-workspace history invalidation, no setting or recreation. Majority retention comes from global-history models deliberately excluded by item-1.2 per-output history; COSMIC has no history verb. |
| G-05 | CHANGE zero to unset per axis: equal (640,0)/(0,480) is not fixed under `both-axes-fixed`, but the nonzero fixed axis qualifies under `either-axis-fixed`. Deliberate COSMIC deviation for consistent zero handling (raw-equality COSMIC floats; 5/8 non-scrolling plus PaperWM/karousel tile). Shared-core/KDE delivered offline; Windows handoff pending. |
| G-06 / G-D1 | CHANGE both platforms to COSMIC: directional focus no-ops while focused-maximized (leave via unmaximize or Alt+Tab); directional move unmaximizes first then moves. REQ-MAX-08 NORMATIVE; Windows parity text replaced, G-D1 resolved. Existing pointer refusals unchanged; shared opt-in focus fence/KDE delivered offline (one clear attempt, observed-clear same-invocation continuation with fresh guards); Windows implementation pending. |
| G-07 | KEEP override reset on restart; COSMIC agrees for unpinned workspaces, persisting only pins we lack. ADD Future pinned/persistent workspaces: empty-surviving with monitor affinity, pinned tiling override survives restart, grouped with shared restart store; stale comparison backlog link fixed. |
| G-37 | KEEP COSMIC last-remaining default; ADD `Source workspace after migration`: `Last remaining workspace` (default, tooltip COSMIC) / `Most recently used workspace` (tooltip bspwm, i3, awesome). KDE `migrationSourceRefill` / Windows `core.migration_source_refill`, functional IDs `last-remaining-workspace` / `most-recently-used-workspace`. MRU uses pre-mutation item-1.2 per-output previous stable ID, eligible while live in the remaining source scoped ring (surviving empty valid, migrated/removed/out-of-scope excluded), otherwise last remaining; preserve a still-scoped live current view. Destination/invalidation unchanged. REQ-WS-12d cross-refers to REQ-WS-12e. Shared selector/KDE KCM/live setting delivered offline; Windows schema/UI/runtime pending. |
| G-D2 | SELECT cross-platform maximized workspace send keeps maximize on arrival (existing Windows policy; Hyprland/qtile/awesome agree). Deliberate COSMIC/niri deviation: sending relocates the whole window/state, unlike an in-layout move. KDE flag-stable native carry delivered offline without restore/remaximize or overlay geometry writes; real native arrival preservation pending. Fullscreen Table A 2026-10-07 and KDE fullscreen observe-first unchanged. |

## MATERIAL (impact order)

### G-01. Equal-sentinel and full-zero guards (REQ-SPC-04a D1)

Rule: [REQ-SPC-04a](../spec/functional-spec.md) (:349),
[fixed-size D1](../decisions.md) (:425-435). Current: whole-vector
guards reject full-zero and unbounded sentinels; equal partial-zero
counts as fixed. D1 calls its default COSMIC without acknowledging any
deviation; the 8/8 non-scrolling contrary evidence below materially
undercuts that claimed parity.

- Sentinel leg (min=max=INT_MAX): D10 C,H,B,I,X,S,Q,A,N,K
  (all float raw equality; K verified both backends,
  `S-kar-spc`); A1 P (tiles everything, no fixed branch); U1 R
  (role-gated, no hint counterpart); T0. Total 1+10+1+0 = 12.
- Full-zero leg (min=max=(0,0)): A8 H,I,S,Q,A,N,P,K (tiles);
  D2 B,X (floats); T1 C (Wayland tiles, X11 present-zero floats);
  U1 R. Total 8+2+1+1 = 12. COSMIC's X11 disagreement is another
  uncovered guard difference, not dismissed as an ordinary single outlier;
  the other seven non-scrolling profiles split A5/D2 in favor of our guard.
- Host semantics: the KWin sentinel is an unbounded marker
  (no-maximum), not an explicit positive equal size; the scenario
  labels it cross-platform raw-equal, so each ref floats it while
  Ours normalizes it to non-fixed. Genuine fixed windows keep no-touch
  admission (D8), so floating is the low-write path and tiling is the
  write path.
- Options: (a) keep D1 rejection (stands against 8/8 + COSMIC;
  sentinel windows receive tiled allocations with writes; genuine fixed
  no-touch untouched); (b) follow majority/COSMIC (float sentinels,
  retaining their native frames without tiling writes); (c) adapter
  normalizes the KWin unbounded marker before core while preserving
  genuine fixed-size no-touch (host adaptation, needs design).
- Review recommendation (decided above, G-01): retain normalization for genuine host-unbounded markers
  and full-zero bounds, explicitly documenting the host adaptation rather
  than claiming exact COSMIC parity. Confirm that choice against the ten
  sentinel-float references before implementation; it must not normalize away
  genuinely fixed positive hints. Option (c) makes the host/core boundary
  explicit but needs design; option (b) follows raw equality instead.

### G-02. Disconnect/return vs approved displacement (REQ-OUT-06)

Rule: [REQ-OUT-06](../spec/functional-spec.md) (:282),
[displacement](../decisions.md) (:183-207). Current: preserve-aside
(never merge), show relocated R workspace with its active focus,
nearest survivor (primary/order fallback), automatic origin return,
moved IDs lose auto-return. Ours native journey TBD both platforms.

- Shown leg: A1 I (shows moved focused workspace); D6 C (L keeps
  showing + L-MRU fixup), X (keeps current positional), Q (hides,
  L keeps showing), A (evacuates to first tag), B (retains in place,
  no evacuation), N (keeps L showing; affinity agrees separately);
  U1 K (single-screen); T4 H,S,P,R (shown node unrecorded/host).
  Total 1+6+1+4 = 12.
- Destination leg: D4 C,H,I,S (first-remaining / primary-first /
  priority-order, not nearest); U5 X,Q,A,B,K (no output-destination
  choice: positional / hide / tag-granularity / retain / single-screen);
  T3 N (sole-survivor fixture undiscriminating), P,R (host).
  Total 0+4+5+3 = 12.
- Affinity leg: A5 C,H,N,S,B (return/priority affinity; B same-RandR
  reuse); D4 I,Q,A,X (fresh/positional assignment); U1 K; T2 P,R.
  Total 5+4+1+2 = 12.
- Uncovered COSMIC shown + destination differences; no coherent
  alternative (retain vs merge vs hide vs fresh-assign).
- Options: (a) keep nearest + relocated-show + affinity; (b-e) any
  single ref model; (f) no setting.
- Review recommendation (decided above, G-02): keep (a) under the documented displacement policy
  (stronger intent than the COSMIC default): merging destroys
  displaced layouts; retain-in-place strands workspaces; fresh
  assignment loses return affinity. Consequence: stands against COSMIC
  shown-L and 6 shown-leg differers; nearest-vs-first-remaining and
  the native journey are unproven live.

### G-03. Sole-window next send fills pre-transfer E, COSMIC avoids it (REQ-WS-14)

Rule: [REQ-WS-14](../spec/functional-spec.md) (:167), item 2.2
([workspaces](../decisions.md) :275-279). Current: scoped ordinal ring,
resolve once, filling trailing E pulls next spare from normal
lifecycle, follow default + stay alternative.

- Sole-next leg ([R-WS-20](../spec/reference-outcomes/workspaces.md)):
  Ours fills pre-transfer E. COSMIC avoids E on every inferred chord
  (Left/Right early return; Up/Down `InvalidWorkspaceIndex`; exact
  next-leg outcome TBD but the avoid-E policy is sourced,
  `S-cos-wssingle`; pin status of the invoked chord is F, which does
  not erase the policy leg). A6 B,I,S,N,P,K (fill E; P conditional on
  E existing in GNOME's scope); D2 C,R (R: both legs no-moves);
  T1 H (numeric+1 may be E or a newly created workspace);
  U3 X,Q,A (no relative-send verb). Total 6+2+3+1 = 12.
- Options: (a) keep ring fill (stands against COSMIC alone on this
  leg); (b) follow COSMIC chord-branching.
- Review recommendation (decided above, G-03): keep (a): item 2.2 NORMATIVE explicitly selects
  "sending into it fills it," which is stronger documented intent
  than the COSMIC default. Consequence: Up/Down-inferred single-window
  sends fill E where COSMIC refuses; exact COSMIC next-leg outcome
  stays TBD (chord F), recorded honestly.

### G-04. History retention vs invalidation (REQ-WS-08/12i)

Rule: [REQ-WS-08](../spec/functional-spec.md) (:152) +
[REQ-WS-12i](../spec/functional-spec.md) (:165), items 1.3/1.5 + D9
([workspaces](../decisions.md) :247-266,:310-327). Current: stable IDs;
removed/unassigned/out-of-scope clear; toggle no-ops; disconnected
history discarded; reconnect never consults history. Items 1.3/1.5
select invalidation but never call reference retention deliberate,
so they do not settle this.

- Out-of-scope history after migration ([R-WS-26](../spec/reference-outcomes/workspaces.md#r-ws-26-history-hotplug-invalidation)):
  A1 B (drops moved-desktop history); D5 H,S,Q,A,N (retain entries;
  A can select the moved tag on R, N's off-monitor ID no-ops);
  T2 I,P (recorded refill/live-MRU differs, but invalidation of the
  moved entry is not established); U4 C,X,K,R (no history verb).
  Total 1+5+4+2 = 12. Non-scroll: A1 D4 T1 U2, a 4:1 applicable
  majority against clearing. This does not cast bspwm's hotplug
  monitor retention as previous-view history or call awesome unsupported.
- Removed-empty recreation ([R-WS-16](../spec/reference-outcomes/workspaces.md#r-ws-16-previous-after-the-visited-empty-workspace-is-removed)):
  D3 H,I,S (recreate by stored number/name); T1 P (E-removal depends
  on GNOME); U8 C,X,K,R (no verb), B,Q,A,N (E survives in this
  fixture, so removal cannot be exercised). Total 0+3+8+1 = 12.
  COSMIC is unsupported on both legs. Hotplug R-WS-17 additionally
  proves retained out-of-scope entries in H/I/S/N; its final targets
  remain qualified by each profile's global/per-output history.
- Options: (a) keep invalidation; (b) follow retain-majority;
  (c) keep clearing as default, offer a functionally named
  `Previous workspace scope` setting: `Current output only` /
  `Follow surviving workspace` (reference names in tooltips).
- Review recommendation (superseded by G-04 above; no setting): keep (a) with a precise distinction: removed-name
  recreation (H/I/S) creates a new workspace identity, contrary to
  the explicit no-recreation rule. Offer (c) for the meaningful
  surviving-but-moved alternative, with stable-ID lookup and no
  recreation; N-style stale-ID no-op is harmless but offers no user
  benefit. Consequence: default still rejects the 4:1 migration
  majority; opt-in permits a previous-view toggle to change output.

### G-05. Equal partial-zero floats in Ours+COSMIC, tiles in 5/8 (REQ-SPC-04a)

Rule and setting: same D1 as G-01. The delivered both/either-axis
setting keeps identical zero guards, so it does not resolve this leg.

- Partial-zero leg ((640,0)/(0,480)): A3 C,B,X (float raw equality);
  D7 H,I,S,Q,A (tile on gates), P,K (tile: no fixed branch /
  enforced-minimum mapping); T1 N ((640,0) tiles, (0,480) floats:
  one vote cannot cover both variants); U1 R. Total 3+7+1+1 = 12.
  Clear non-scrolling majority (5v3) with COSMIC agreeing.
- Options: (a) keep float (COSMIC default); (b) follow tile majority;
  (c) no new setting (this is a correctness guard, and the existing
  axis setting intentionally leaves guards untouched).
- Review recommendation (superseded by G-05 above): keep (a) under the COSMIC default; consequence:
  stands against 5 non-scrolling + P/K tiling. User may choose (b);
  that choice redefines equal-partial-zero as non-fixed everywhere.

### G-06. MAX-08 focus permit + unresolved maximized-move suppression (REQ-MAX-08)

Rule: [REQ-MAX-08](../spec/functional-spec.md) (:219, OPEN) linked to
[G-D1](#record-drift) below. Current text records Windows
focus-permit + maximized-move-refusal while status stays unselected.

- Focus leg ([R-MAX-08](../spec/reference-outcomes/maximize-fullscreen.md),
  judged against current PERMIT, not fence): D3 C,H,A (fence by
  early-return / fullscreen-cycle-off / occlusion-miss); A3 Q (wraps
  A-to-C), N (column step, no fence), K (lands on A, clears overlay);
  U6 B,I,X,S (no maximize counterpart), P (width-only conversion),
  R (no native leg). Total 3+3+6+0 = 12. Non-scroll 3 fence vs 1
  access; full 3v3 includes 2 scrolling unfencers.
- Move leg: the sequence attempts focus first, so it is not a
  maximized-mover isolation test. Comparable action-on-maximized-B:
  H refuses (agrees with Windows-refuse direction), C unmaximizes
  then moves B (differs), A swaps B with no tile change (differs);
  Q moves C and N/K/P move A (never exercise a maximized mover).
  Tally: A1 H (refuses); D2 C,A (unmax-move / swap);
  T4 Q,N,K,P (maximized mover unexercised); U5 B,I,X,S,R.
  Total 1+2+4+5 = 12. No invented maximized-mover test claimed.
- Q was not counted as maximized-refusal agreement (mover C is not
  maximum); N/K moves of A were not counted as focused-maximum
  refusal differences.
- Options focus: (a) keep permit; (b) follow C/H/A fence; (c) unfenced
  setting (Q lone non-scroll -> skip per single-outlier rule).
  Options move: (a) keep Windows refusal; (b) follow COSMIC
  unmax-move (single ref).
- Review recommendation (superseded by G-06 above; G-D1 resolved): resolve G-D1 first (rule OPEN). Then fence focus
  per COSMIC default (C/H/A; consequence: directional focus cannot
  enter/leave a maximized tile, Q + 2 scrolling unfencers opposed).
  Keep move refusal (consequence: stands against COSMIC unmax-move
  alone; moving a maximized window has no majority semantics).

### G-07. Pinned override persists in COSMIC, Ours resets (REQ-CTL-04)

Rule: [REQ-CTL-04](../spec/functional-spec.md) (:326),
[workspaces](../decisions.md) (:148-151). Current: overrides reset on
owner restart. COSMIC `S-cos-persist` keeps the pinned override
(differs; fixture pin status unstated so exact outcome TBD, but the
persist-policy leg is sourced). No recorded coverage.

- Tally: A0 D1 C; U11 H,B,I,X,S,Q,A,N,P,K,R (each explicitly no
  pinned-override counterpart); T0. Total 0+1+11+0 = 12.
- Options: (a) keep reset (session-local; re-pin every restart);
  (b) follow COSMIC (persist pinned overrides; needs a cross-restart
  persisted workspace configuration). No setting (single ref).
- Review recommendation (superseded by G-07 above; current reset retained, pins Future): follow (b) under the COSMIC-default rule, subject
  to user decision. Consequence of (b): new
  persisted state + pin-semantics design; consequence of (a): stands
  against COSMIC alone.

### G-37. Migration source refill and destination order (REQ-WS-12d/e)

Rule: [REQ-WS-12d/e](../spec/functional-spec.md) (:160-161),
[workspace migration D4/D5](../decisions/workspaces.md#workspace-lifecycle-focus-and-history). Current:
insert after target current; show the migrated workspace; source shows
the last remaining scoped entry, with normal spare maintenance.

- Source-refill policy ([R-WS-23](../spec/reference-outcomes/workspaces.md#r-ws-23-source-refill-empty-migrate)):
  A1 C (last remaining); D7 H (first-enumerated/first-free plug),
  B (history-last/head), I (focus-head), S (stacked/first remainder),
  Q (group swap), A (history restore), N (previous entry);
  T1 P (GNOME source lifecycle); U3 X,K,R.
  Total 1+7+3+1 = 12. Non-scroll A1 D6 U1: clear majority differs
  from COSMIC, but no single replacement mechanism has that majority.
  H/B/A exact source identities remain fixture-qualified; this tally
  compares selection policies, not final identities.
- Destination-insertion leg ([R-WS-22](../spec/reference-outcomes/workspaces.md#r-ws-22-active-migrate-layout-retained)):
  A2 C,N (after current); D1 H (in-place creation order);
  T4 B,I,S,P (target insertion order unresolved);
  U5 X,Q,A,K,R (no comparable per-output insertion ordering).
  Total 2+1+5+4 = 12. H alone would not warrant a setting.
- Options: keep last-entry refill; follow a specific majority member
  (no common majority policy); or offer `Source workspace after migration`
  with `Last remaining workspace` (COSMIC tooltip) / `Most recently used
  remaining workspace` (bspwm/i3/awesome tooltip), while retaining the
  destination insertion rule.
- Review recommendation (selected by G-37 above): keep the COSMIC last-entry default and offer the MRU
  alternative if selected by the user. This is a meaningful non-scrolling
  history-based split, rather than a new platform-specific mode. Consequence:
  opt-in can choose a different source view after moving its active workspace;
  it needs a defined eligible-history/fallback rule before implementation.

## NOT MATERIAL (compact table)

| ID | Rule leg | 12-col tally (order CHBIXSQANKPR) | Why not material |
|----|-----------|----------------------------------|------------------|
| G-08 | MOV-08 ordering, R4 exclusion [decisions](../decisions.md) :1635-1647 | A H,I,S,A,B,R; D C,Q,N; T X,P; U K | Recorded all-direction no-cycle exclusion settles it; B occupied cross + N stay counted, not TBD |
| G-09 | WS-19 wrap | A C,B,I,S; D H,N,P,K,R; U X,Q,A | Only H differs non-scrolling (MRU-invalid); rest scrolling/user-host |
| G-10 | Spare lifecycle (WS-18/20) | A C,N; D B,I,S,K; T H,P; U X,Q,A,R | COSMIC agrees (ensure-spare); thin 3-ref retain direction; item 2.2 intent stands; not bundled with target/follow |
| G-12 | DRAG center-join | D C(join),X(raw),B,A(hover),N(commit); T rest | Join covered by standing V-GROUP-STACK deferral ([REQ-GRP-02](../spec/functional-spec.md), refused to post-0.1); hover-swap 2 refs, no auto-setting |
| G-13 | FLT-01 fresh unfloat | A C,H,I,S,Q,N,K,R; D B,X,A; U P | COSMIC + majority agree; 3 retained-slot outliers, no setting |
| G-14 | FLT-01 first-float placement policy | D N(tile+offset); T C,H,S,I,X,Q,A,K,R; U B,P | Exact frames TBD in every compared cell; no approved compared frame to contradict (V-FLOAT-GEO provisional) |
| G-15 | FLT-06 retention (B9) | A C,Q,N-outcome; D H,A,K; U B,X,S; T I,P,R | COSMIC agrees; B9 selected; 2-2 split, rest no counterpart |
| G-16 | FLT-08 miss retains | A C,H,I,X,S,N,K,R; D B,A,Q; T P | Retain majority + COSMIC; 3 cross-layer outliers |
| G-17 | FLT-09 far float | A C,H,I,X,S,N,R; D B,A,Q,K; T P | 5/8 + COSMIC agree across 4 families |
| G-18 | FLT-10/11 snap | A C,H; D B,I,X,S,A,N,P,K,R; U Q | COSMIC agrees (KDE half-snap delivered); 9 non-snap alternatives incoherent |
| G-19 | MAX-01 slot overlay | A C,H; D Q,A; U B,I,X,S,P,R; T N,K | COSMIC agrees; 4 no-counterpart non-votes; Q3 selected |
| G-20 | MAX-02 keep-tree | A H,B,I,S,Q,N,P,K; D C,X,A,R | Documented platform policy: KWin owns cover-and-restore; retain allocation with no fullscreen geometry writes ([decisions](../decisions/window-state.md#fullscreen-and-maximize-overlays) :1418-1427). No new evidence undercuts that host policy; paneru's separate fullscreen strip removes the reserved column |
| G-21 | MAX-06 Q3 reserved slot | A C,H; D Q,A,K; T N; U B,I,X,S,P,R | COSMIC agrees; N born-max column/restore is traced but tree-slot comparison is model-qualified; Q3 selected |
| G-22 | MAX-09 arrival-overlay carry (separate max/full legs) | Max: A H,Q,A,K; D C,N; U B,I,X,S,P-width; T R-host. Full: A C,H,B,I,X,S,Q,A,K; D N; T P,R | Windows fullscreen carry accepted Table A 2026-10-07; maximize preservation is recorded Windows policy (G-D2 wording drift). COSMIC agrees on full, clears max; niri-only full difference is scrolling. H clears/re-applies state during transfer, so arrival agreement does not prove a no-touch journey; KDE observe-first remains TBD |
| G-23 | One-axis-fixed admission vs both-axis default | A C,B,I,X,Q,A,P,K; D S; T H,N; U R | Both/either setting already delivered. H Wayland floats / X11 tiles; N width-fixed tiles / height-fixed floats. Those splits do not match both default variants; guards split to G-01/G-05 |
| G-25 | SPC-08 hint-change admission-only | A C,H,B,I,X,S,Q,N,P,K; D A; U R | Single-outlier dynamic reclassification; D2 selected |
| G-26 | SPC-10 fixed born-max base | A C,H,B,I,S,Q,A,X,K; D N,P; U R | Ours = COSMIC + majority (float beneath overlay); 1 scrolling mechanism + 1 strip-model case |
| G-27 | SPC-11 first exit | A H,B,I,S-outcome; D C-born-tiles,X-tiled; T Q,A,N,P,K; U R | D5 explicitly records the COSMIC game-safety deviation; exit outcomes otherwise converge |
| G-28 | RST-01 float persist | A B,X,A; D Q,I; T C,P,K; U H,N,S,R | Thin split, COSMIC journey TBD; H/N have no in-profile owner-restart journey; preserve accepted Table A |
| G-29 | SPC-13 override restart | A B,X,A; D Q,I-fresh-loss; T C,P,K; U H,N,S,R | Owner-specific state formats; D7 persistence selected |
| G-30 | WS-01 shipped follow default | A C,H,Q,N,P; D B,I,X,S,A,K; T R | Follow-default and send-and-stay alternative explicitly decided 2026-10-07; meaningful split already addressed |
| G-31 | Send admission chain | A C; D B,I,S,X,A,Q,N; T H,P,K; U R | COSMIC agrees; 7 differ across 6 incoherent mechanisms; follow/stay decided |
| G-32 | WS-02 return at A | A C,B,I,X,S,Q; D N-active+1; T H,A,P,K; U R | Outcome converges (MRU/live/parent/insertUp); mechanism-only entries |
| G-33 | WS-04 fallback chain | A C,H,I; D X,B,N; T S,Q,A,P,K,R | Scenario TBD; COSMIC + sole-candidate agree; 3 incoherent |
| G-34 | WS-12 hidden reach | A C-active; D I,S,Q,A; T B,H,N,K,R; U X,P | COSMIC active-direction agrees; 4 incoherent extras |
| G-35 | WS-24 sticky stays on source | A C,H,B; D I,S,A; T P; U X,Q,N,K,R | D7 explicitly selects COSMIC per-output ownership; guard/scope difference is settled. P scratch stays outside workspace membership but native visibility is qualified |
| G-36 | WS-21 project capability-refusal gate | U C,H,B,I,X,S,Q,A,N,P,K,R | Shared/false/unreadable project refusal states have no equivalent reference gate; migration availability itself is not a vote against a KWin capability check |
| G-38 | WS-27 tie-break | D H,I-first-enum; T C,B,S,N,P; U X,Q,A,K,R | Two known first-enumerated alternatives; other applicable selector ties untraced. Largest-edge then left/top explicitly selected 2026-10-09; no new rationale-undermining evidence |
| G-39 | OUT-01 focus lands | A H,I,S,B; D A-peer; T C,X,Q,N,P; U K,R | Single-outlier peer-focus/retag; COSMIC is authored observation, not a source vote; metric deliberate (G-40) |
| G-40 | Window-based selection (MOV-12) | A H,I,S,A,B; D C-origin,X-hybrid; T Q,N,P; U K,R | Deliberate User 2026-10-09 selection; N stays is a differ on crossing, not the metric; scroll named individually |
| G-41 | Empty-target eligibility | D B-noop; T C,H,I,X,S,Q,A,N,P,R; U K | Single-ref boundary; crossing accepted Table A |
| G-42 | INS-01 axis | A C,B,H; D X,A; T Q; U I,S,N,P,K,R | COSMIC agrees (long-edge user statement); axis split, newcomer position OPEN. U6 = I,S,N,P,K,R exactly |
| G-43 | CLOSE-01 MRU | A C,H,B,I,S,Q,A,K; D X; T N,P,R | Single positional outlier; M-majority |
| G-44 | MOU-02 shared edge | D N-model; T C,H,B,I,S,Q,A,R; U X,P,K | Single scrolling model case |
| G-45 | FLT-02 effective float-only guard | A C,H,I,S; D B,A; U X,Q,N; T P,K,R | 4v2, no majority against the rule; COSMIC auto-floats while H refuses and I/S flags are ineffective on tiles. The single-output fixture cannot tally output scope; source-scoped policy is settled in D7 |

## RECORD DRIFT

| ID | Review-baseline inconsistency | Resolution 2026-10-10 |
|----|---------------|--------------|
| G-D1 | [REQ-MAX-08](../spec/functional-spec.md) (:219) OPEN/unselected vs [decisions](../decisions.md) :1458-1461 Windows text (focus permit, maximized-move refusal) + backlog:1678 "no R-MAX-08 nav choice" | Resolved by G-06: NORMATIVE COSMIC focus fence/unmaximize-before-move on both platforms; shared/KDE delivered offline, Windows gap queued, prior Windows parity text replaced. |
| G-D2 | [REQ-MAX-09](../spec/functional-spec.md) (:220) maximize-unresolved vs [decisions](../decisions.md) :1461-1462 Windows maximize preservation; Table A separately accepts Windows fullscreen carry | Resolved: cross-platform maximize preservation selected with deliberate COSMIC/niri deviation; KDE delivered offline, native arrival preservation pending. Fullscreen Table A and KDE fullscreen observe-first remain separate and unchanged. Linked behavior: G-22. |

## Ledger (87 entries, unambiguous)

C8: MOV-08-order G-08 (+metric G-40); MAX-08-focus G-06; MAX-08-move
G-06; MAX-09 G-22; CTL-04 G-07; WS-20 G-03 (+spare G-10 context);
SPC-07 G-01 (+G-05 partial-zero leg); OUT-06 G-02 (3 legs).
X11: CLOSE G-43; INS-01 G-42; WS-02/04+OUT-04/07 G-31+G-32+G-33;
MOV-08 G-08 (+G-40); FLT-01 G-13; MAX-02 G-20; MAX-06 G-21; SPC-07
G-01+G-05; SPC-11 G-27; OUT-06 G-02; DRAG G-12.
A11: INS-01 G-42; WS-01/02+OUT-07 G-30+G-31; OUT-01 G-39 (+G-40);
OUT-06 G-02; FLT-01 G-13; MAX-01/02/03 G-19+G-20; MAX-06 G-21;
SPC-08 G-25; DRAG-01 G-12; FLT-10/11 G-18; WS-12 G-34.
B8: FLT-01 G-13; MAX-02-focus G-20 (focus sub-leg Ours-TBD,
non-voting); WS-01/02/04+OUT-07 G-30+G-31+G-33; OUT-06 G-02;
sticky G-45; MOV-08/11/12+OUT-01/04
G-08+G-40+G-41+G-39; DRAG-01/07 G-12; SPC-07 G-01+G-05.
H8: FLT-06 G-15; OUT-06 G-02; WS-16 G-04; WS-19/20 G-09+G-03 (+G-10);
WS-17/26 G-04; WS-21 G-36; WS-22/23 G-37; WS-27 G-38.
Q10: FLT-08 G-16; FLT-09 G-17; MAX-01 G-19; MAX-06 G-21; WS-02 G-32;
OUT-06 G-02; WS-12 G-34; WS-17 G-04; RST-01 G-28; SPC-13 G-29.
N11: FLT-01 G-14+G-13; SPC-04a G-23 (+G-05); SPC-10 G-26; SPC-11 G-27;
WS-02 G-32; WS-04 G-33; WS-17 G-04; WS-18/20 G-03 (+G-09/G-10);
OUT-07 G-31; DRAG-01 G-12; MOU-02 G-44.
S9: OUT-06 G-02; WS-17 G-04; WS-20 G-03 (+G-10); WS-12 G-34; SPC-04a
G-23+G-05; FLT-01 G-14+G-13; SPC-11 G-27; WS-12/D7 G-35; OUT-01
G-39 (+G-40).
I11: SPC-13 G-29; OUT-06 G-02; WS-12 G-34; WS-17 G-04; WS-20 G-03
(+G-10); WS-27 G-38; SPC-04a G-23+G-05; FLT-01 G-14+G-13; SPC-11 G-27;
WS-12/D7 G-35; OUT-01 G-39 (+G-40).
Sum: 8+11+11+8+8+10+11+9+11 = 87. Composite splits name distinct legs
(admit/place, sentinel/partial/zero, ordering/metric/eligibility,
move/focus, target/spare/wrap, behavior/drift); no issue
is double-counted.

## Coverage, ambiguities, checks

- Coverage 87/87; scrolling formal entries 0 (three notes state no
  conflicts; three commits are cell-fill deliveries). Material 8
  (G-01..G-07 and G-37), NOT MATERIAL 35 (G-08..G-45 except G-11/G-24/G-37), RECORD
  DRIFT 2 (G-D1/D2): 45 rule groups total. G-11 was merged into G-04;
  G-24 full-zero normalization was merged into G-01 to retain COSMIC's
  backend-specific guard difference in the material review;
  a separate sticky-scope group was removed because the fixture cannot
  discriminate output scope. IDs are retained for stable review references.
- Open ambiguities: G-06 move leg unexercised for Ours by the matrix
  sequence (no maximized-mover test invented); G-07 pin semantics F
  even in COSMIC (policy leg still clear); G-03 COSMIC exact next-leg
  outcome TBD (avoid-E policy still clear); G-01 COSMIC X11
  full-zero floats while Wayland tiles.
- Arithmetic: every compared leg accounts for all 12 named references;
  source-entry ledger sums to 87. Independent verification covers all
  eight material groups and a non-material/drift sample. Coverage,
  material tallies, classifications and recommendations passed after
  source-grounded reconciliation; no live testing was used.
