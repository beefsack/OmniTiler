# Reference-WM consensus across the full matrix (analysis note)

Date: 2026-10-07. Base: main HEAD `b942020`.
Matrix: [reference-outcomes matrix](../spec/reference-outcomes.md) (155 rows).
The tables below retain the 125-row historical audit baseline. Later
USER-decision discriminators were excluded when their reference outcomes
were TBD; established new legs are counted in the
[2026-10-09 source-fill supplement](#pinned-source-fill-supplement-2026-10-09).
Profiles/legend: matrix profile and legend sections. Families (4, per user):
COSMIC n-ary; binary (Hyprland dwindle + bspwm); tree (i3 + sway, correlated);
layout-driven (xmonad + qtile + awesome, correlated triple).
Source passes: [bspwm/xmonad](../changes/archive/reference-wm-bspwm-xmonad-source.md),
[qtile/awesome](../changes/archive/reference-wm-qtile-awesome-source.md).
Decisions re-read: [decisions](../decisions.md) (cross-platform, window and
workspace behavior, COSMIC movement and groups), [post-Windows audit](cross-platform-core/post-windows-audit.md),
[KDE float nav](../changes/archive/kde-floating-directional-navigation.md),
[workspace tiling](../changes/archive/windows-workspace-tiling.md),
[alt-tab](windows-port/alt-tab-hidden-workspaces.md).

Scope: classify all 125 rows from matrix cells only (58-row historical
audit plus expansion sections, each preserved); no product code, matrix,
decision, or record edits; no live testing.

## Evidence rule

A cell votes only if the source establishes the requested action's outcome on
an applicable fixture. `E` (no vote): EU no counterpart in the profiled
source; EO owner-specific/external; EG/EF/EN geometry, anchor/focus-identity,
or native journey TBD on the discriminating point; ET fixture inapplicable;
EP partial leg (votes for that leg only); ER recorded user test or authored
observation without an `S()` tag (non-voting qualifier). Missing maximize
state (EU) never votes: ordinary-manage reports from maximize-stateless
profiles are feature absence, not deliberate overlay rejection. Compound
predicates split into legs with explicit sub-leg voter lists.

Strength rule (user rule, no added floor): Strong = 5+ agreeing of 8, or
all-but-one of those with evidence; strong cross-family needs agreeing WMs
from 3+ of the 4 families. One row may place sub-legs in different tables;
table counts are per-predicate, not disjoint row partitions. Numerical
majority/all-but-one rows that fail cross-family breadth go to the weak
table, never called "no consensus" unqualified. 1-1 ties and 1/1 singletons
are not consensus (no floor needed, breadth disclosed).

Match vocabulary per platform (KDE / Windows): yes, no, partial, unknown
(offline, live-accepted, TBD). Audit Result points at summary tables
(A/B/U/C/W).

Ground truth applied (user, not reopened): B6 minimum-infeasible is
origin+minimum on BOTH platforms (KDE delivered offline). Q3 born-maximized is
reserved slot + maximize overlay (selected). B9 overlaid intentional unfloat
unmaximizes then fresh-admits (User 2026-10-08; KDE/shared delivered offline);
refusal sub-leg 4/4 no-refusal aligns B9, contradicts current Windows refusal.
COSMIC confirmation and KDE native timing remain pending, not decision gates.
KDE float focus + half-snap delivered and live-accepted at 9de7274.
Stateful quarter-snap stays deferred.

## Full audit (58 rows; codes in C H B I X S Q A order)

C=COSMIC, H=Hyprland, B=bspwm, I=i3, X=xmonad, S=sway, Q=qtile, A=awesome.

| Row | Row-local classes | C H B I X S Q A | ev/noev | lead | Result |
|---|---|---|---|---|---|
| R-INS-01 | axis L long-edge, F flat-H, V vertical-stack; position after/before | L L L F V F V V | 8/0 | axis tie L3/V3 | U position after-5/7ev (C,B,I,S,A vs X,Q; H EF); C axis |
| R-INS-02 | J joins, N ordinary-admits | J J N J EU J N N | 7/1 | J4 | C audit-only |
| R-MOV-01 | R restructure, O noop, N in-column/swap only | ER EG O R EG R N N | 5/3 | tie R2/N2 | C audit-only |
| R-MOV-02 | W swap | ER W W W W W W W | 7/1 | W7 | B 7/7 sourced (COSMIC observed ER) |
| R-MOV-03 | W nested wrap, F flat swap, C column-carry | ER EG F F ET F C F | 5/3 | F4 | A (COSMIC observed wrap ER; H EG, X ET) |
| R-MOV-04 | E escape, N binary-retain, P partial extract, C carry, S swap/miss | ER N N P ET P C S | 6/2 | tie N2/P2 | C audit-only |
| R-MOV-05 | O noop | ER O O O O O O O | 7/1 | O7 | B 7/7 sourced (COSMIC observed ER) |
| R-WS-01 shipped-default send | F follows, S stays | F F S S S S F S | 8/0 | S5 | A shipped-default leg (stay-5/8, 3/4 fam; ours follows both; alternates inventoried, never voted) |
| R-WS-02 | A returns at A; order after/before; axis qualifier | A EG A A A A A EG | 6/2 | A6 | B anchor; U after-4/5ev (C,B,I,S vs X); axis qualifier |
| R-WS-03 | R reuses | R EU EU EU EU EU EU EU | 1/7 | - | C audit-only (singleton, not consensus) |
| R-WS-04 | D lands at surviving D, O other anchor (X at live C) | D D EF D O D EF EF | 5/3 | D4 | U (ours unknown) |
| R-WS-05 | T fresh-readmit, R float retained | T R R R R R R R | 8/0 | R7 | U (ours unknown) |
| R-WS-06 | M mode-clamped-origin, P per-tag unarranged | M EU EU EU EU EU EU P | 2/6 | - | C (1-1 tie, not consensus) |
| R-WS-07 | L listed plus switch | L EO EO EO EO EO EO EO | 1/7 | - | C (singleton, not consensus) |
| R-FLT-01 | F fresh admission, S same-slot | F F S F S F F F | 8/0 | F6 | B fresh-vs-oldslot leg only (anchor/order/frame differ) |
| R-FLT-02 | V visible, K flag-but-stays, R refused | V R V K EU K EU V | 6/2 | V3 | C audit-only |
| R-FLT-03 | R ratio preserved | ER EG EG R EG R EG EG | 2/6 | R2 | W (2/2 one family) |
| R-FLT-04 | T re-admit, P stateless recalc | T EU EU EU EU EU EU P | 2/6 | - | C (1-1 tie, not consensus) |
| R-FLT-05 | V sticky-visible; X sticky leg inapplicable | EN EO V V ET EU EO V | 3/5 | V3 thin | A (3/3ev, 3 families; native journey TBD) |
| R-FLT-06 | U unmax-then-admit, R retain, P proceed; I/B/X/S no native-max fixture | U R EU EU EU EU U R | 4/4 | refusal NO4; retention 2-2 | A refusal 4/4ev; C retention 2-2 |
| R-FLT-07 | T tile A | T T T T T T T T | 8/0 | T8 | B unanimous |
| R-FLT-08 | R retained, T enters tile | R R T R R R T T | 8/0 | R5 | B (COSMIC cell only, prose config-bound, see note) |
| R-FLT-09 | G focuses G, T focuses tile | G G T G G G T T | 8/0 | G5 | A (KDE live-accepted, Win no) |
| R-FLT-10 | Hf half-snap, Ee edge-snap, W swap, Px pixel, No noop | Hf Ee W Px No Px EU W | 7/1 | tie | C audit-only |
| R-FLT-11 | Q stateful quarter, S stateless | Q S EF S S S EU S | 6/2 | S5 | B (COSMIC differs) |
| R-MAX-01 | S slot overlay, M mode-obscure, L leaves tiling | S M EU EU EU EU L L | 4/4 | L2 | C audit-only |
| R-MAX-02 | K kept, Rm removed+remap, Rr removed+readmit | Rm K K K K K Rr Rr | 8/0 | K5 | B (COSMIC differs) |
| R-MAX-03 | F fresh-admit-all, H hold-in-floating | F EU EU EU EU EU EU H | 2/6 | - | C (1-1 tie, not consensus) |
| R-MAX-04 | N new attempt, S backend-split | N N EU EU EU EU S N | 4/4 | N3 thin | B thin (ours both new-attempt, physical TBD) |
| R-MAX-05 | N no refusal | N N N N N N N N | 8/0 | N8 | A unanimous (Win no) |
| R-MAX-06 | O overlay/state, F implicit float | O O EU EU EU EU EU F | 3/5 | O2 | W (2 agreeing families, thin) |
| R-MAX-07 | T ordinary tile, no size inference | T T T T T T T T | 8/0 | T8 | A unanimous (Win differs) |
| R-START-01 | N nested chain, P per-tag recalc | N EU EU EU EU EU EU P | 2/6 | - | C (1-1 tie, not consensus) |
| R-START-02 | N nested chain, P per-tag recalc | N EU EU EU EU EU EU P | 2/6 | - | W no-centre-cut sub-point 2/2ev, 2 families |
| R-START-03 | U ignores minima, S hint-shaped | U EU EU EU EU EU EU S | 2/6 | - | C (1-1 tie, not consensus) |
| R-CLOSE-01 | M MRU/stack/history, S spatial default, P positional | M S M M P M P M | 8/0 | M5 | B |
| R-CLOSE-02 | F fresh admission, no old-slot | F F F F F F F F | 8/0 | F8 | U (ours unknown) |
| R-GRP-01 | T single-tab, G one-window group, P parent retarget | T G EU P EU P EU EU | 4/4 | P2 | C (P tree-pair only) |
| R-OUT-01 | C cross, N noop | ER C C C EF C N C | 6/2 | C5 | B (COSMIC observed ER) |
| R-OUT-02 | W local wrap, C cross, L local split | ER EG C W ET W L C | 5/3 | tie W2/C2 | C audit-only |
| R-DRAG-01 | J join, W swap, M centre-move, F float-out | J F W M F W W W | 8/0 | W4 | C (4/8 not majority) |
| R-DRAG-02 | F bar-index, N stays, Ei edge-insert, Fl floats, Cd conditional | F EG N Ei Fl Cd N N | 7/1 | N3 | C audit-only |
| R-DRAG-03 | S same topology | S EF EF S EF S EF S | 4/4 | S4 thin | B thin |
| R-DRAG-04 | D drop/persist, C cancel-revert | D D D C D D D D | 8/0 | D7 | A (ours differs) |
| R-DRAG-05 | N no mutation, R retile, F floats | N R N N F N N N | 8/0 | N6 | B |
| R-DRAG-06 | N nopark, P parks off-area | N N N N P N N N | 8/0 | N7 | B |
| R-DRAG-07 | F follows pointer, R retained | F F R R F R F R | 8/0 | tie 4-4 | C exact split |
| R-DRAG-08 | F focus at press, N no focus write | F F N N F F F F | 8/0 | F6 | U (ours unknown/unknown) |
| R-CTL-01 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-CTL-02 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-CTL-03 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-CTL-04 | owner-selected (COSMIC config leg only) | EN EU EU EU EU EU EU EU | 0/8 | - | C no-evidence row |
| R-CTL-05 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-CTL-06 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-CTL-07 | owner-only | EO EO EO EO EO EO EO EO | 0/8 | - | C no-evidence row |
| R-MIN-01 | U default-unclamped alloc, S hint-shaped | U U U U U U U S | 8/0 | U7 | A (ours min-enforced both; see MIN note) |
| R-MIN-02 | same | U U U U U U U S | 8/0 | U7 | A (same) |
| R-MIN-03 | same; all 8 tile the oversized sole (no auto-float) | U U U U U U U S | 8/0 | U7 | A (same; bspwm opt-in origin clamp off default) |

Coverage: 125/125 rows audited (115 below plus 10 column rows in the
column section), eight reference classifications per row.
Summary counts including insertion/focus/move/resize/layout/workspace/minimize/maximize/groups/floating/close/multi-output/mouse/special-windows/activation/restart/column are per-predicate: A 24, B 20, U 19 full rows + two KDE legs, C 19, W 11;
multi-leg rows overlap, and the full audit also covers unrelated rows.
Table A is the single consolidated differences list (24 rows: A23 plus
the R-RSZ-01 Windows keyboard-trigger gap, whose KDE match stays in B).

## Table A: strong cross-family consensus where ours differs (24, consolidated)

Ours differs = established follow/refusal/etc on at least one platform,
or an inventory-evidenced verb/state gap on both platforms (missing verbs
never vote as agreement). User decisions 2026-10-07 are recorded in the
Decision column; the selected deliberate divergences retain selected policy;
consensus never overrides standing decisions. R-WS-01 records the user's 2026-10-07
selection; all 24 rows have a decision or pending-check annotation.
R-FLT-06 waits on the COSMIC live check; R-FLT-09 retains existing Windows parity.
Items 1-5 detail annotations below refine eight selected rows without changing
their reference votes or asserting implementation evidence.

| Row | Consensus | Count | COSMIC | Ours KDE / Windows | Deliberate vs feature absence | Recommendation | Decision 2026-10-07 |
|---|---|---|---|---|---|---|---|
| R-MOV-08 | exhausted vertical move crosses to the output above | cross-5/8 (H,B,I,S,A), 3/4 fam; Q stays, C/X TBD | TBD | no / no (Up/Down excluded from R4) | Deliberate directional output policy; existing V-R4-DIR exclusion, user reconsideration queued | Revise to cross after local movement is exhausted | Accepted 2026-10-07: cross after exhaustion |
| R-WS-01 send | shipped-default binding stays (leaves source focus) | S5/8, 3/4 fam (B,I,X,S,A) | no (shipped MoveToWorkspace follows; SendToWorkspace stays is the unbound alternate) | follow / follow (recorded verified-transfer policy) | Deliberate command semantics; profile/binding-dependent | Retain selected follow; separate send-and-stay decided 2026-10-07 | Decided 2026-10-07: follow default plus send-and-stay |
| R-MOV-03 | flat swap, not same-orientation nested wrap | F4/5ev, 3 fam | observed wrap ER, no source vote | no / no (Engine R2c wrap; fixture-qualified) | Deliberate swaps across models, not exact topology parity | Retain selected R2c wrap | Decided 2026-10-07: COSMIC wrap default, flat-swap setting |
| R-FLT-05 | restart retains sticky visibility | V3/3ev thin (B,I,A), 3 fam | unknown | yes / no (normal-float marker consumption); native journey TBD | Deliberate state restoration; missing sticky/restart fixtures excluded | Establish native sticky/restart journey first | Decided 2026-10-07: sticky survives restart with R-RST-01 |
| R-FLT-06 refusal | no refusal of overlaid-unfloat toggle | NO4/4ev (C,H,Q,A), 3 fam | yes (unmax-then-admits) | yes-allows / no-refuses (selected B9 is no-refusal) | Deliberate toggle paths; absent native-max fixtures excluded | Resolve Windows refusal against selected B9 direction | Pending 2026-10-07: awaits COSMIC B9 live check |
| R-FLT-09 | float-origin right focuses far float G | G5/8, 4/4 fam | yes | yes live-accepted / no (subject refusal; parity pending) | Deliberate focus policies; dissent mixes cross-layer search and absent float-only search | Establish Windows float-subject parity | Accepted 2026-10-07: existing Windows parity |
| R-MAX-05 | no refusal of app-owned fullscreen toggle | N8/8 | yes | yes (public setter) / no (preimage-gate refusal, standing decision) | Deliberate fullscreen paths, distinct from missing maximize | Retain standing preimage-gate refusal | Decided 2026-10-07: keep refusal, later spike |
| R-MAX-07 | no size inference; caption cover tiles | T8/8 | yes | yes / no (Windows containment classifies fullscreen) | Deliberate state-based classification | Classify by window state, not inferred containment | Decided 2026-10-07: keep Windows inference |
| R-MIN-01 | profile-default tiled alloc does not enforce minima | U7/8 | yes | no / no (B6 min-enforced both; KDE skip is the gap) | Default/absent tiled clamps; H/B opt-in clamps disabled | Retain selected B6 | Decided 2026-10-07: keep B6, no setting |
| R-MIN-02 | same on shrink | U7/8 | yes | no / no (same B6 split) | Same default-clamp policy, not settled native frames | Retain selected B6 | Decided 2026-10-07: keep B6, no setting |
| R-MIN-03 | same, oversized sole tiles | U7/8 | yes | no / no (flag + skip/raise gap) | Same clamp policy; tile admission itself is unanimous | Retain selected B6 | Decided 2026-10-07: keep B6, no setting |
| R-DRAG-04 | Esc does not cancel (drop/persist) | D7/8 | yes | no / no (cancel verdict selected; i3 revert is the outlier) | Escape handling mixes normal drop and no cancellation path | Retain selected cancel verdict | Decided 2026-10-07: keep Esc cancel |
| R-LAY-01 | parent split orientation flips, same children on the new axis | flip-4/5ev (C,B,I,S vs H geometry-reset no-op), 3/4 fam | yes (ToggleOrientation) | no / no (no orientation verb in any Engine/adapter layer) | Inventory gap, not a rejecting policy; X/Q/A have no counterpart either | Add parent-axis toggle | Accepted 2026-10-07: add parent-axis toggle |
| R-LAY-04 | alternative layout selected at per-workspace scope, order preserved | scope-6/8 (B,I,S,X,Q,A), 3/4 fam | n/a (global config only) | no / no (no select verb in any Engine/adapter layer) | Ownership differs per profile (desktop/parent/workspace/group/tag); H partial, C global-only | Park until a genuine second layout exists | User decision 2026-10-09: PARK / OPEN; revisit when tabbed stacks are designed after 0.1; whether tabs count as L2 remains a future decision |
| R-WS-08 | previous-view toggle | toggle-5/8 (B,I,S,Q,A), 3/4 fam; H named variant only | no counterpart | no / no (missing history verb) | Inventory gap; PaperWM MRU traversal is non-voting | Add previous-workspace toggle | Accepted 2026-10-07: add previous-workspace toggle |
| R-WS-11 edge | relative workspace switch wraps at native inventory ends | wrap-6/8 (C,B,I,S,Q,A), 4/4 fam | yes (default on) | no / no (missing relative-switch verb) | Inventory gap; primary WS3 can be mid-inventory | Add relative switch on the edge leg | Accepted 2026-10-07: add wrapping relative switch |
| R-WS-12 hidden transfer | whole workspace/group/tag reassigned to another output | reassign-5/8 (H,B,I,Q,A), 3/4 fam; destination/focus partial | active-only variant | no / no (missing whole-workspace verb) | Inventory gap; counts semantic domain reassignment, not identical mechanisms | Add whole-workspace reassignment | Accepted 2026-10-07: add whole-workspace move |
| R-WS-14 | relative workspace send exists | relative-5/8 (C,H,B,I,S), 3/4 fam; follow policies differ | yes | no / no (missing relative-send verb) | Inventory gap; H previous is MRU, not numeric decrement | Add relative send | Accepted 2026-10-07: add relative send |
| R-MAX-09 full | fullscreen window carries its state to the target workspace (not restored first) | carry-5/8 (B,I,S,Q,A), 3/4 fam; H/C TBD; niri window-send strips (scrolling, non-voting) | TBD (transfer sourced, carry untraced) | KDE TBD / Windows refuses fullscreen, carries maximized | Windows refusal differs; KDE native-send outcome untraced; max leg has no consensus | Carry fullscreen state on Windows send; KDE native-send outcome first | Accepted 2026-10-07: Windows carry, KDE observe first |
| R-GRP-03 close | closed active tab leaves a retained 2-tab group focused on C | retained-4/4ev plus focus-C-4/4ev (C,H,I,S), 3/4 fam; B/X/Q/A qualified | yes (active tab C) | no / no (no tab carrier and no close-tab verb in any Engine layer) | Inventory gap under standing V-GROUP-STACK deferral; foreign direction recorded if tabs ever specified | Retain V-GROUP-STACK deferral | Decided 2026-10-07: tabs first after 0.1, close keeps group focuses next |
| R-OUT-04 send | explicit output transfer carries; declared follow forms follow the mover | carry-8/8 and follow-7/8 (C/H/B/I/S/Q/A), 4/4 fam; X target-stack focus only, source refocus TBD | yes (MoveToOutput follows, SendToOutput stays) | no / no (no send verb in any Engine/adapter layer; directional CrossOutput move is the separate R-OUT-01 verb) | Verb-shape gap, not a rejecting policy; send vs directional-move verbs distinguished | Add explicit output-send verb via shared Engine | Accepted 2026-10-07: add explicit output send |
| R-SPC-04 fixed | fixed-size window floats instead of tiling | float-8/8 (C,H,B,I,X,S,Q,A), 4/4 fam | yes (min==max is dialog) | tiles / tiles-with-hint-clamp (Normal kind observed; no fixed-size exception) | Type-exception gap; Ours KDE native clamp TBD | Add fixed-size float admission | Accepted 2026-10-07: float fixed-size on admission |
| R-RST-01 float | ordinary float status survives the owner restart | recover-4/4ev (B/I/X/A), 3/4 fam; Q float outcome TBD | TBD (native restart journey) | no / no (intentional floats reset: session-local id set on KDE, session-local float store cleared on stop on Windows) | State persistence gap; preserve intentional-float identity across owner restart | Preserve intentional-float identity across restart | Accepted 2026-10-07: preserve floats across restart |
| R-RSZ-01 Windows | explicit pixel-step grow/shrink path | pixel-5/8 (C,H,B,I,S), 3/4 fam; H/B partial for neighbor/reversal | yes | KDE matches (stays B) / Windows no keyboard trigger (`S-ours-winbind`) | Inventory gap, not a rejecting policy | Add Windows keyboard-resize trigger via shared Engine pixel path | Accepted 2026-10-07: Windows keyboard resize |

Additional decisions 2026-10-07 outside Table A
([recorded selections](../decisions.md#window-state-float-sticky-maximize-fullscreen),
[drag selections](../decisions.md#pointer-drag-and-drop)):

- R-MAX-03 Q3 scope: keep maximize over a reserved slot on floating-to-tiled
  admission, replacing one-shot restore; implementation pending on both platforms.
- R-DRAG-07: follow host-native drag presentation where available; keep KDE
  pointer-following and Windows title-bar native movement. Windows Win+drag
  keeps its stationary source plus moving preview unless native-loop delegation
  is nearly free.
- R-DRAG-08: Meta/Win client drag focuses the mover at press on both platforms;
  Windows implementation pending, KDE timing needs a live check.

### Detailed USER selections 2026-10-07: reference-consensus additions

[Recorded decisions D1 and items 1-2](../decisions.md#workspaces),
[items 3-5](../decisions.md#move-layout-and-output-commands),
[implementation order / Windows handoff](../backlog.md): shared Rust core +
KDE adapter are implemented KDE-side, Windows adapter wiring follows;
Windows build/behavior breakage acceptable when specific repairs are in the
handoff, extended by each implementation piece. All eight rows below have
selected targets, implementation pending; annotations are not reference votes.

- R-WS-08 (item 1): Meta/Win+Ctrl+Tab two-view toggle, stable previous ID.
  Local/global-unique per-output history, shared one history. Every successful
  observed change records (commands/native/verified follow/hotplug), not
  same-workspace activation/output focus. Surviving empties valid;
  removed/unassigned/out-of-recording-output-scope IDs clear, no-op until next
  change, no reinterpretation/recreation. Decision 1.5: reconnect selection
  never consults/restores history; hotplug records normally; disconnected
  output history discarded (session-local output identity). R-WS-15..17.
- R-WS-11 (item 1): Meta/Win+Ctrl+arrows and +H/J/K/L, left/up previous,
  right/down next (COSMIC parity). Same mode scope; ring contains all existing
  ordered workspaces, including trailing empty and ordinals beyond 9, wraps
  first/last, selection creates nothing. Authentic takes conflicting holders;
  Compatible disables conflicting arrows, letters remain (KDE KWin desktop
  switch, Windows native Left/Right general knowledge, unverified in repo).
- R-WS-01 (item 2): keep numbered follow Meta/Win+Shift+digits;
  numbered/relative stay commands bindable, unbound. Follow/stay applies to
  absolute/relative sends including floating boundaries. Existing-default
  application: KDE currently preserves source view at floating boundaries;
  item 2 routes explicit intent so default follows, not a new follow decision.
- R-WS-14 (item 2): relative follow Meta/Win+Ctrl+Shift+arrows and +H/J/K/L;
  ordinal previous/next in R-WS-11 ring, not MRU, wrapping including trailing
  empty; resolve once before transfer. Filling trailing empty gets next spare
  from normal lifecycle. Authentic clears KDE KWin window-desktop arrow
  holders; Compatible disables our arrows, letters remain; Windows ownership
  unknown. Discriminators R-WS-18..20 (stay and emptied-source fresh runs).
- R-MOV-03 (item 3): global KDE `sameAxisMove` / Windows
  `core.same_axis_move`, `group-with-neighbor` default or `swap-with-neighbor`.
  Functional rename delivered offline 2026-10-08; WM names appear in tooltips only.
  Windows additive
  schema-v1 field, missing defaults to wrap; subsequent moves change without
  tree rebuild; KDE UI control. Flat-swap replaces R2c only for adjacent direct
  leaf siblings in the same group, shares travel with windows; leaf/group rules
  unchanged, TBD rows before broadening. R-MOV-09/10.
- R-LAY-01 (item 4): Meta+O / Win+O immediate-parent toggle including root,
  order/shares/focus preserved; sole leaf no-op, no saved admission hint,
  long-edge unchanged. No KDE stock holder found; Win+O orientation lock:
  Authentic takes over, Compatible disables. R-LAY-05/06.
- R-MOV-08 (item 5): local restructure/swap/escape first, then cross all
  four directions, sole root leaf too (horizontal gate also changes).
  Reciprocal edge-touch + positive overlap on FULL output rectangles
  (horizontal too), not work areas; panel gaps do not block. Window-based
  selection (User decision 2026-10-09): shared edge containing the moving
  window centre projection, else larger window-span overlap, final left/top
  tie-break; unreadable topology refuses, no candidate no-op, no output wrap.
  Prior shared core/KDE delivery used the replaced refusal; new selection
  pending code. R-MOV-11..13.
- R-OUT-04 (item 5): explicit follow/stay send; follow
  Meta/Win+Ctrl+Alt+arrows and +H/J/K/L, stay bindable unbound. KDE arms absent
  from `kglobalshortcutsrc`, Windows ownership unknown; COSMIC's Shift+Alt arm
  collides with resize-shrink, niri's with item 2. Same output selection as
  R-MOV-08, destination current workspace; ordinary send remembered-leaf,
  focus-history, root admission and command follow/stay. Initially tiled
  eligibility, sticky excluded; floating boundaries membership-only, tiled
  sides reflow; ordinary float transfer remains OPEN. R-OUT-07.

WS-01 notes: original-eight shipped-default bindings follow in 3/8
(C MoveToWorkspace, H window.move with follow absent, Q togroup
switch_group=True) and stay in 5/8 (B node -d without --follow,
I/S move-to-workspace, X W.shift, A move_to_tag). Evidenced
extensions: niri follows (move-column-to-workspace focus=true
default) and PaperWM follows on shipped take/move completion (drop
into the selected space plus `Main.activateWindow`,
tiling.js:5528-5555 at 8bf6dd2, `S-pap-take`; accepted). Ten
evidenced profiles (original eight + niri + PaperWM) tally 5 follow /
5 stay; legacy inactive-space-insertion no-steal (`S-pap-ins`) is a
different journey, not the shipped-send outcome. karousel/paneru
remain qualified (partial focus/position). Modern Wayland
(COSMIC, Hyprland, niri) follows while i3/sway/bspwm/xmonad/awesome
stay; qtile follows. Alternates: COSMIC SendToWorkspace stays;
Hyprland follow=false stays; bspwm --follow follows; qtile
switch_group=False stays; niri focus=false stays; i3/sway independent
`workspace` switch after move; xmonad W.greedyView . W.shift composition;
awesome move_to_tag then view_only. All original eight offer both policies
through verbs, flags or command compositions; niri does too. PaperWM's
send-and-stay alternate remains unestablished. Alternates are inventory
only, never consensus votes. Ours keeps follow plus a separate send-and-stay
(decided 2026-10-07).

## Table B: strong consensus ours matches (20; pending explicit)

| Row | Consensus | Count | COSMIC | Ours KDE / Windows |
|---|---|---|---|---|
| R-RSZ-01 pixel path | explicit pixel-step grow/shrink path | pixel-5/8 (C,H,B,I,S), 3/4 fam; H/B partial for neighbor/reversal | yes | yes KDE (Engine pixel path, `S-ours-resize`) / no-counterpart Windows (missing keyboard trigger, `S-ours-winbind`, not rejecting policy; Windows leg consolidated as Table A R-RSZ-01) |
| R-FOC-02 edge stay | single-output edge focus retains (no wrap, no fallback) | stay-5/8, 3/4 fam (C,H,B,X,A) | yes (no local target, no next output) | yes / yes (Edge refuse, no dispatch) |
| R-INS-03 empty admission | first open tiles at full work area, newcomer focused | 8/8, 4/4 fam | yes | partial-offline (allocation + desired focus; native activation TBD) |
| R-WS-02 anchor | return lands at A | A6/6ev (C,B,I,X,S,Q), 4/4 fam | yes (target MRU) | yes (remembered A) |
| R-FLT-01 | unfloat is fresh admission (old-slot excluded only) | F6/8, 4/4 fam | yes | yes / yes |
| R-FLT-07 | tile-origin focus lands on tile A | T8/8 | yes | yes / yes |
| R-FLT-08 | float-origin miss retains F | R5/8, 4/4 fam | yes-cell (prose config-bound, see note) | yes-offline (live TBD) / yes-outcome via subject refusal (mechanism partial) |
| R-FLT-11 | no stateful quarter | S5/6ev | no (stateful quarter) | yes stateless-half (stateful deferred) / unknown |
| R-MAX-02 | fullscreen keeps tree, no node removal | K5/8, 3 fam | no (deliberate remove+remap) | yes / yes (focus sequence pending) |
| R-MAX-04 | repress after native restore is a new attempt | N3/4ev thin | yes | yes / yes (physical TBD) |
| R-MOV-02 | move-up swaps in place | W7/7 sourced | observed ER | yes / yes |
| R-MOV-05 | single-output edge is noop | O7/7 sourced | observed ER | yes / yes (offline) |
| R-OUT-01 | occupied-target move crosses outputs | C5/6ev, 3 fam | observed ER | yes-offline / yes-offline |
| R-OUT-03 | exhausted left focus crosses; target X | cross-6/8 (B/I/X/S/A + H partial), target-X-5/8 (B/I/X/S/A), 3 fam; Q stays, C fall-through TBD | branch TBD (workspace step vs output switch under shipped layout) | yes / yes (Engine proposal selects X; adapter actuation) |
| R-CLOSE-01 | close refocus follows MRU/stack/history rule | M5/8, 4/4 fam | yes | yes / yes |
| R-DRAG-03 | both producers share one tiled-drag topology | S4/4ev thin, 3 fam | yes | yes / yes-synthetic (exact row TBD) |
| R-DRAG-05 | zero-move press/release does not mutate | N6/8, 4/4 fam | yes | yes / yes |
| R-DRAG-06 | no off-area parking | N7/8 | yes | yes / yes |
| R-WS-09 return | restores remembered workspace focus | remembered-7/8 (C,B,I,X,S,Q,A), 4/4 fam; H pointer-dependent | yes | TBD shell-driven / yes (`last_focus`) |
| R-FLT-13 ordinary-hidden | ordinary float stays on its workspace (hidden while away) | hidden-8/8, 4 fam | yes (per-workspace floats) | TBD native journey / yes-offline (hide/reveal) |

FLT-01 qualifier: anchors differ (MRU vs Dwindle vs after-focus vs
position/order); consensus covers fresh-vs-oldslot only. WS-02 after-order
leg (after-4/5ev C,B,I,S vs X before) is U (ours exact order TBD); tall/wide
axis stays a qualifier (X/Q inapplicable).

## Table U: strong consensus, ours unresolved (19 full rows + two KDE legs)

Note: tallies below are the dated historical audit snapshot. The
R-OUT-05 focused-7/8 count is map-only (static-pointer fixture) and is
superseded for motion-first legs by the
[cursor-first supplement](#cursor-first-output-targeting-supplement-2026-10-20);
do not read it as current motion-first evidence. It selects nothing.

| Row | Consensus | Count | COSMIC | Ours |
|---|---|---|---|---|
| R-INS-01 position | newcomer after focus | after-5/7ev (C,B,I,S,A vs X,Q; H EF), 4/4 fam | after (sourced) | order TBD / TBD |
| R-WS-02 after | B after A | after-4/5ev (C,B,I,S vs X before) | after | exact order TBD |
| R-WS-04 | B lands at surviving D (history vs sole-candidate) | D4/5ev, 3 fam | yes | unknown / unknown (selected history rule authorized, scenario TBD) |
| R-WS-05 | floated B transfers retaining float | R7/8 | no | unknown / unknown (roundtrip untested) |
| R-CLOSE-02 | reopen is fresh admission, no old-slot | F8/8 | yes | unknown / unknown (not checked) |
| R-DRAG-08 | Meta/Win press focuses mover | F6/8, 4/4 fam | yes | unknown / unknown (KDE timing TBD; Win drop-activate only, press-focus unproven) |
| R-WS-09 KDE leg | restores remembered workspace focus | remembered-7/8, 4/4 fam | yes | KDE shell-driven return TBD; Windows match recorded in B |
| R-FLT-13 KDE leg | ordinary float hidden on workspace switch | hidden-8/8, 4 fam | yes (per-workspace floats) | KDE native select journey TBD; Windows match recorded in B |
| R-FLT-12 raise path | an F-raising path exists (focus/activate/press/verb) | raise-7/8 (C,H,B,I,S,Q,A; X layer-only), 4 fam | yes (focus raise) | order TBD both (host stacking); lower has only weak agreement |
| R-FLT-14 free frame | pointer drag/resize keeps the free frame | free-8/8, 4 fam | yes (floating move/resize grabs) | project resize refuses; host journey TBD on both platforms |
| R-CLOSE-03 sole | shown workspace retained on sole close | retained-7/8 (C,B,I,X,S,Q,A), 4/4 fam | yes (active kept) | TBD both (Engine collapse sourced; native journey TBD) |
| R-CLOSE-04 float | tiles untouched with MRU refocus | untouched-6/8 plus focus-B-6/8 (C,B,I,S,Q,A), 4/4 fam | yes (layer separation plus MRU fixup) | TBD both (exception-drop sourced; adapter journey TBD) |
| R-CLOSE-05 full | closed fullscreen needs no restore; survivor refills with focus | full-leg-5/8 (H/B/I/Q/A), 3 fam | TBD (removal sourced, overlay cleanup TBD) | TBD both (removal plus desired focus sourced; native cleanup TBD) |
| R-OUT-05 open | ordinary admission lands on the focused output with newcomer focus | focused-7/8 (C,B,I,X,S,Q,A), 4/4 fam; HISTORICAL map-only tally, superseded for motion-first legs by the [cursor-first supplement](#cursor-first-output-targeting-supplement-2026-10-20) | yes (active-output default plus mapfocus) | routing TBD both (admission anchor sourced) |
| R-MOU-01 click | plain click focuses B | click-8/8, 4/4 fam | yes (press focuses) | TBD both (host click/hover journeys; adapter verbs prove no producer path) |
| R-SPC-01 transient | transient dialog floats instead of tiling | float-8/8 (C,H,B,I,X,S,Q,A), 4/4 fam | floats (is_dialog) | KDE TBD (`normalWindow` dialog mapping untraced); Windows excludes owned dialogs | Eligibility gap, not an established difference |
| R-SPC-05 hints | app-owned resize/hint change leaves allocation authoritative | ignore-8/8 (C,H,B,I,X,S,Q,A), 4/4 fam; A later arrange hint-shaping qualifier | yes (no minimum enforcement) | authoritative both (KDE/host reaction and exact reflow TBD) | Direction matches; remainders queued |
| R-ACT-02 urgency | urgency marker without steal, cleared on focus | mark-7/8 with evidence (5E + C/A partial), 4 fam; clear-5/8 (H/B/I/S/Q), 3 fam | sandboxed workspace marker, same-workspace clear TBD | TBD both (native mark/clear journeys; no attention signal in either adapter inventory) | Establish the host mark/clear journey before any change |
| R-RST-01 set | workspace set recovered across the restart | recover-5/8 (B/I/X/Q plus A partial), 3/4 fam | n/a (host set persists independently) | unknown / unknown (set host-owned, never restored by the tiler) | Establish set handling before any persistence decision |
| R-RST-01 membership | tiled windows land back in their workspaces | recover-3/3ev (B/I/X), 3/4 fam; Q/A placement TBD, never a differ vote | n/a | unknown / unknown (membership re-observed, never restored) | Establish Ours membership recovery before any persistence decision |
| R-RST-01 focus | focused window recovered across the restart | recover-3/3ev (B/I/X), 3/4 fam; Q TBD, never a differ vote | n/a | unknown / unknown (native focus TBD on both) | Establish Ours post-restart focus before any persistence decision |

DRAG-08: unknown is not mismatch; B/I have deliberate no-focus paths.

## Table C: listed COSMIC differences without strong consensus (19)

| Row | COSMIC vs ours (recorded) | Reason |
|---|---|---|
| R-INS-01 axis | same (long-edge; order TBD both) | L3 vs F2 vs V3 full-axis split (position leg is U above) |
| R-WS-02 axis | same (remembered A both) | long-edge vs live-split vs parent vs inapplicable; no axis consensus |
| R-WS-06 | same (mode exists both) | 1-1 tie (mode-clamped-origin vs per-tag unarranged); 6 EU |
| R-WS-07 | partial (KDE same; Win omits) | singleton (COSMIC path only); 7 EO |
| R-FLT-02 | differs (output-set layer vs all-ws scope) | visible 3 vs flag-but-stays 2 (tree pair) vs refusal 1; X,Q EU |
| R-FLT-04 | same (toggle exists both) | 1-1 tie; 6 EU |
| R-FLT-06 retention | partial (KDE allows, Win refuses) | unmax-admit 2 (C,Q) vs retain 2 (H,A); refusal leg is A above |
| R-FLT-10 | partial (KDE half-snap matches; Win refuses) | half/edge/swap-2/pixel-2/noop split; Q EU |
| R-MAX-03 | partial (fresh-admit direction; ours one-shot clear) | 1-1 tie; 6 EU |
| R-START-01 | same direction (nested chain) | 1-1 tie; 6 EU |
| R-START-03 | differs (ignores minima vs B6) | 1-1 tie on handling; 6 EU |
| R-GRP-01 | differs (stack vs deferred/refuse) | parent-retarget 2 (tree pair) vs singletons; 4 EU |
| R-DRAG-01 | differs (join vs refuse) | swap 4/8 (not majority) vs join 1 / centre 1 / float-out 2 |
| R-DRAG-07 | partial (KDE follows; Win stationary) | exact 4-4 split |
| R-CTL-04 | N/A (config leg only) | 0 full-predicate votes; 7 EU |
| R-WS-10 | KDE owner-specific; Windows retained vs COSMIC removed | remove-3 vs retain-4, H destruction TBD |
| R-WS-13 | KDE select owner-specific; Windows refuses like COSMIC | create-3 vs refuse/no-op-3, Q/A absent-WS9 fixture impossible |
| R-SPC-02 types | Ours KDE eligibility TBD; Windows utility excluded, splash TBD | Different splash/utility admission; focus/switcher remainders, no strong predicate |
| R-SPC-03 PiP | Ours app-specific eligibility TBD | App/version/flags/rules required; no universal PiP type or original-eight outcome votes |

Resolved without contradicting consensus: DRAG-01 refusal and GRP-01
deferral selected; START-02 long-edge chain provisional follows COSMIC;
FLT-10 KDE half-snap delivered matches COSMIC; WS-03 Windows reuse matches
COSMIC.

## Table W: numerical but not strong cross-family (11; weak, disclosed)

| Row | Numerical result | Breadth | Status |
|---|---|---|---|---|
| R-RSZ-02 outer edge | no-op 3/4ev (C,I,S vs Q redistribution) | 2 agreeing families (COSMIC, tree) | weak; KDE matches, Windows no keyboard counterpart |
| R-RSZ-03 nested scope | nearest inner split 3/3ev (C,I,S) | 2 families (COSMIC, tree) | weak; KDE matches, Windows no keyboard counterpart |
| R-FLT-03 | ratio-preserve 2/2ev (I,S; COSMIC UT is ER) | 1 family (tree pair) | weak; ours unchecked |
| R-MAX-06 | overlay/state 2/3ev (C,H vs A implicit-float) | 2 agreeing families | thin; Q3 selected matches overlay direction, recorded one-shot-clear differs; no strong contradiction either way |
| R-START-02 sub-point | no-centre-cut 2/2ev (C,A) | 2 families | weak sub-point only; chain itself 1-1 |
| R-FOC-01 tie | MRU-sensitive 3/4ev (B,I,S vs C stable) | 2 agreeing families (binary, tree) | weak; Ours stable A-both matches COSMIC |
| R-FOC-03 order | next C, previous B 4/4ev (I,S,Q,X) | 2 families (tree, layout-driven) | weak sub-leg; Ours no cycle counterpart |
| R-FOC-04 scope | parent group then remembered child 3/3ev (C,I,S) | 2 families (COSMIC, tree) | weak; Ours leaf-only, no counterpart |
| R-MNZ-01 allocation | minimized window leaves tiling allocation with a restore path 3/3ev (C,Q,A) | 2 families (COSMIC, layout-driven) | weak; i3 refusal is non-voting, H/S/X TBD; Ours KDE TBD, Windows retains |
| R-MNZ-02 restore slot | restore returns the old slot 2/2ev (C,A) | 2 families (COSMIC, layout-driven) | weak thin; Q path-only, H/S/X TBD; Ours Windows matches, KDE TBD |
| R-FLT-12 lower | explicit lower moves F to its layer bottom 2/2ev (Q,A) | 1 family (layout-driven) | weak; absent lower verbs non-voting, Ours no project lower path |

FLT-05 notes: A EWMH roundtrip (sticky reads on every selected tag) counts
as source policy like B/I full-state restores (exact journeys TBD in all
three); X ordinary-float carry is no-evidence because the sticky predicate
has no counterpart there. Q re-manage outcome stays EO.

MIN note: the counted conflict is default tiling-minimum policy (no
enforcement under profile defaults) vs selected B6 min-enforced behavior. It
is not source proof that settled native minima are ignored (exact
shrink/grow frames TBD in every cell). MIN-03 admission itself is unanimous
tile (no auto-float in any profile); the counted U7/S1 split is
minimum-clamp policy only (A hint-shaping; H/B opt-in clamps off default).
Fixed-size (min==max) float admission is recorded in cells but was not
counted as a discriminator here.

FLT-08 note: the COSMIC cell proves retain-on-miss for one output with no
next output. No-workspace-cycling generally comes from section prose
(Vertical layout routes missed horizontal focus to output fallback):
prose-tier, config-bound to that layout, disclosed here. The matrix does
not separately establish a cross-WM workspace-cycling predicate, so that
path has no counted consensus here.

Decision conflicts flagged (scope makes no decisions): B6 vs MIN rows;
selected R2c wrap vs MOV-03 (fixture-qualified); cancel verdict vs DRAG-04;
Windows preimage refusal vs MAX-05 and selected B9 no-refusal direction;
Windows containment vs MAX-07. MAX-06 thin 2/3 support is no strong
contradiction of Q3 in either direction.

## Limitations

- Thin/low-evidence passes (FLT-05 3/3, MAX-04 3/4, DRAG-03 4/4, Table W rows) are
  marked as such, never full 5+ weight.
- i3/sway-only legs never carry cross-family alone (FLT-02, FLT-10,
  GRP-01, OUT-02 wrap leg).
- awesome per-tag partials (WS-06, FLT-04) vote as their own distinct
  class, not toward the COSMIC outcome.
- Recorded-ours vs target gaps: WS-04/WS-05/CLOSE-02/INS-01-order/WS-02-order/
  DRAG-08 scenarios TBD; FLT-11 Windows pending; MAX-02/04 and DRAG-03/06
  exact journeys TBD; Windows float-subject parity pending (FLT-08/09/10).

## Insertion expansion (piece B1): R-INS-03..08 plus R-INS-01/02 scrolling backfill

Scope: piece B1 adds six GWT insertion scenarios (R-INS-03..08) and
additive scrolling backfill blocks for R-INS-01/02. Historical tables and
the 58-row audit above are preserved unchanged. Denominator, families, and
the strength rule are unchanged: consensus classification below counts the
original eight profiles only. The four scrolling profiles (niri, PaperWM,
karousel/Lazy, paneru) form one correlated lineage reported as an explicit
separate non-voting comparison; they never silently redefine a denominator.
Cell classes for the expansion are mutually exclusive per cell: E
complete outcome evidenced; P at least one predicate sub-leg evidenced
with the remainder TBD; T TBD-only (branch notes cited for non-coverage
do not promote a cell to P); Q all requested legs qualified
(fixture-inapplicable / no-counterpart / owner-specific, each with pinned
inventory evidence); M mixed qualified and applicable-TBD legs. Branch
evidence is not a full-fixture vote. Sub-leg discipline: topology, anchor,
focus, exact-geometry,
verb-inventory, and routing-mechanism sub-legs split with explicit voter
lists. Partial geometry/focus branch tags are never counted as complete
outcomes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-INS-03 empty admission | first open tiles and focuses | unanimous 8/8, 4/4 fam: every profile tiles A at full work area (partitioning) with newcomer focus (C `S-cos-axis`+`S-cos-mapfocus`, H `S-hyp-ins`+`S-hyp-newfocus`, B `S-bsp-insert`+`S-bsp-ins`, I `S-i3-ins`, X `S-xmo-admit`+`S-xmo-ins`, S `S-sway-ins`+`S-sway-wsdefault`, Q `S-qti-add`, A `S-awe-tile`+`S-awe-manage`) | B unanimous (ours offline-match: single leaf, geometry applied, desired focus; native activation TBD) |
| R-INS-04 chained admission | leg 1 inherits R-INS-01 policy branches only (landscape work area does not prove B at 1200x600, so no dimension is re-voted); legs 2-3 topology/focus/geometry | leg-1 topology stated per profile in the matrix (all 8 leg-1 complete at policy level except Hyprland newcomer side and Ours/scroll remainders noted there); legs 2-3: 0/8 evidenced (live-test route: pointer/focus updates plus repeated recalc); position sub-leg inherits R-INS-01 U | C audit-only |
| R-INS-05 float-focus anchor | anchor exclusion vs fallback; newcomer layer/focus | anchor to B evidenced: C (`S-cos-last` tiling-tree search skips the float), I (floating-con descends to the tiling descendant, `S-i3-ins`), S (focus-inactive tiling anchor, `S-sway-ins`); partial: H (float excluded from the active-tiled candidate, but pointer-hit vs active-tile under shipped follow_mouse leaves B unresolved); Ours differs (fallback wraps the whole root old/new, desired focus newcomer); TBD: B,X,Q,A; newcomer-tiles-focused 4/8 thin (C,H,I,S) | C audit-only |
| R-INS-06 over overlay | overlay retained/cleared/covering; newcomer focus/visibility; underlying layout; separate `B:max` vs fresh `B:full` legs | `B:max` leg no-counterpart: B (no maximize in the state inventory, `S-bsp-fs`), I (`S-i3-max`), X (`S-xmo-layout`), S (`S-sway-max`), all consistent with the R-FLT-06 no-native-max classification; `B:full` legs applicable but TBD in those four; C/H/Q/A legs applicable but TBD | C audit-only |
| R-INS-07 inactive-workspace routing | target-local anchor vs global focus; no focus steal; no output switch | 0/8 complete (routing TBD at pin in all eight) | C audit-only |
| R-INS-08 preselected direction | verb inventory; override vs unsupported; one-shot vs persistent | complete: H (`S-hyp-pre`: preselect verb, forced axis/side, one-shot reset under shipped default); verb+manual-mode: B (`S-bsp-pre`); verb+orientation-set: I (`S-i3-split`), S (`S-sway-default`+`S-sway-split`); no-counterpart: X (`S-xmo-layout` fixed Tall); TBD: C,Q,A (a missing search term is not inventory proof) | C audit-only |
| R-INS-01 scrolling supplement | column admission position/focus/viewport under the separate column Given (no exact H projection; same A/B/C identities) | position-policy branches: niri (`S-nir-ins`: new column after active), PaperWM (`S-pap-ins`: selected+1 RIGHT), karousel (`S-kar-ins`: new column after last-focused), paneru (`S-pan-ins`: rule-index/overlap/end); focus/viewport 0/4 | non-voting comparison only |
| R-INS-02 scrolling supplement | tab join vs ordinary admission under `COL[C1[S[A*,B]]]` with open C (same identities/action as the tree fixture) | non-join position: niri (ordinary open wraps a new column, `S-nir-ins`), karousel (ordinary open a new column, `S-kar-ins`; stacked display exists but off default); active-tab/focus TBD in both; TBD: PaperWM (no tabbed-display column evidenced at pin); fixture-inapplicable: paneru (`Stack` is visible stacking, `Tabs` app-native, `S-pan-model`; exact S fixture has no counterpart, native-tab variant under R-COL-10) | non-voting comparison only |

Counts for this expansion (92 cells: 84 new + 8 backfill; mutually
exclusive E/P/T/Q/M). Original-eight new-row cells (6x8=48): E 12
(R-INS-03: 8; R-INS-05: 3 C/I/S; R-INS-08: 1 H), P 12 (R-INS-04: 8
leg-1-stated with legs 2-3 TBD; R-INS-05: 1 H pointer-vs-B partial;
R-INS-08: 3 B/I/S verb-level), T 19 (R-INS-05: 4 B/X/Q/A; R-INS-06: 4
C/H/Q/A; R-INS-07: 8; R-INS-08: 3 C/Q/A), Q 1 (R-INS-08: X), M 4
(R-INS-06: B,I,X,S each has no-counterpart `B:max`
and applicable-TBD `B:full`). Scrolling new-row cells (6x4=24): P 6
(R-INS-03: 4 position-at-default-width with focus/width remainders;
R-INS-07 niri routing and PaperWM routing+no-steal: 2),
T 18, Q 0, M 0.
Backfill cells (2x4=8): P 6 (R-INS-01: 4 position with widths/focus/viewport
TBD; R-INS-02: niri+karousel non-join: 2), T 1 (R-INS-02: PaperWM),
Q 1 (R-INS-02: paneru fixture-inapplicable per `S-pan-model`). Ours
cells (6x2=12; backfill blocks carry no Ours cells): P 6 (R-INS-03
single-leaf+geometry+desired-focus with native activation TBD: 2; R-INS-04
leg-1 topology+desired-focus with legs 2-3 TBD: 2; R-INS-05
fallback root-wrap+desired-focus with native activation TBD: 2),
T 6 (R-INS-06/07/08: 6), Q 0. Grand totals: E 12,
P 30, T 44, Q 2, M 4 (92 cells). E cells carry no TBD; every P/M cell
names its explicit load-bearing remainder. These cell counts
measure documentation coverage, not consensus votes.

Ours-vs-consensus position (no behavior selected): R-INS-03 establishes
strong consensus (B unanimous) with Ours offline-matching on both
platforms; native activation stays the explicit pending leg. The other
five new rows carry no strong consensus (all C audit-only with explicit
sub-leg voters), so no further ours-vs-consensus conflict is
established. Ours empty-tree single-leaf geometry and desired-focus
branches are sourced; Hyprland preselect is completely sourced (E cell)
while bspwm/i3/sway preselect stay verb-level (P cells); Ours preselect
inventory and admission-over-overlay stay TBD on both platforms. No product choice is
selected by these source findings.

## Focus expansion (piece B2): R-FOC-01..04

Scope: piece B2 adds four GWT focus scenarios (R-FOC-01..04), each with
14 Then profiles. The focus area had no existing rows, so there is no
scrolling backfill to assess (0 existing rows, 0 cells); R-FLT-07..09
tile/float directional-layer coverage is cited by reuse from the
floating area, never duplicated. Historical tables and the 58-row audit
above are preserved unchanged. Denominator, families, and the strength
rule are unchanged: consensus classification below counts the original
eight profiles only. The four scrolling profiles (niri, PaperWM,
karousel/Lazy, paneru) form one correlated lineage reported as an
explicit separate non-voting comparison; they never silently redefine a
denominator. Cell classes are mutually exclusive per cell: E complete
outcome evidenced; P at least one predicate sub-leg evidenced with the
remainder TBD; T TBD-only; Q all requested legs qualified
(fixture-inapplicable / no-counterpart / owner-specific, each with
pinned inventory evidence); M mixed qualified and applicable-TBD legs.
E cells carry no TBD; every P cell names its explicit load-bearing
remainder. These cell counts measure documentation coverage, not
consensus votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-FOC-01 directional tie | A in both runs vs MRU-sensitive choice | A-both: C (equidistant first-minimum tie, history unused: `S-cos-tilefocus`); MRU-sensitive: B (boundary tie, history rank: `S-bsp-flt-focus`), I (sibling plus focus-descend: `S-i3-flt-focus`), S (sibling plus inactive view: `S-sway-focus`); partial: H geometric identity TBD, Q column-current member TBD; qualified: X (flat Tall), A (nmaster-1 tile cannot host fixture) | W MRU 3/4ev, 2 families |
| R-FOC-02 edge focus | wrap vs stay at a single-output edge | wrap to B: I, S, Q (wrapping policies `S-i3-flt-focus`/`S-sway-focus`/`S-qti-focus`); stay on A: C, H, B, X, A (`S-cos-tilefocus`+`S-cos-focus-fallback`, `S-hyp-focus`, `S-bsp-flt-focus`, `S-xmo-nav`, `S-awe-focus`) | B stay 5/8, 3 families |
| R-FOC-03 next/previous cycle | sequential order plus reversibility, wrap (fresh edge leg), float inclusion | complete sequential legs with F excluded: I, S, Q (next C, previous B, edges wrap); partial: X (same stack cycle, float position TBD), A (index wrap, order TBD); qualified: C (no native cycle pair); TBD: H previous invocation/order/wrap/float, B embedding/traversal | W order 4/4ev, 2 families; other legs split |
| R-FOC-04 parent/child scope | container focus vs leaf-only/no-counterpart | container scope: C (Out group / In remembered-child, `S-cos-tilefocus`), I (`S-i3-focuslvl`), S (`S-sway-focuslvl`); no-counterpart: X (`S-xmo-layout`), Q (`S-qti-focus`+`S-qti-split`), A (`S-awe-focus`); TBD (inventory checks queued): H, B | W scope 3/3ev, 2 families |

Order sub-leg: next-C/previous-B is evidenced in I, S, Q (E) plus X
(P main legs) = 4/4 with evidence across two families. It meets the
all-but-one numerical rule, but not three-family breadth, so goes to W.
Qualified/missing outcomes never vote. I/S/Q wrap; X also establishes
wrapping, but its float leg stays TBD; scrolling karousel separately stays
at the edge. These comparisons do not supply Ours with a cycle verb.

Scrolling comparison (non-voting): R-FOC-01 PaperWM run-1-B/run-2-A via
topmost-member pick (`S-pap-focus`) is the only complete scrolling cell;
niri/karousel stay column-level partial (member TBD); paneru stays TBD.
R-FOC-02 three scrolling profiles retain (edge no-op:
`S-nir-focus`, `S-pap-focus`, `S-kar-focus`); paneru stays TBD. R-FOC-03
karousel next-C/previous-B tiled-only is complete; niri and paneru have
no-counterpart (no plain cycle pair: `S-nir-actions`, `S-pan-cmds`);
PaperWM stays TBD. R-FOC-04 niri, PaperWM and karousel have
no-counterpart (no parent verb: `S-nir-focus`, `S-pap-focus`,
`S-kar-focus`); paneru stays TBD on the Stack/Column model per
`S-pan-model`.

Counts for this expansion (56 cells: 4x14; mutually exclusive
E/P/T/Q/M). Original-eight cells (4x8=32): E 18 (R-FOC-01: 4 C/B/I/S;
R-FOC-02: 8 C/H/B/I/X/S/Q/A; R-FOC-03: 3 I/S/Q; R-FOC-04: 3 C/I/S),
P 4 (R-FOC-01: 2 H/Q; R-FOC-03: 2 X/A), T 4 (R-FOC-03: 2 H/B;
R-FOC-04: 2 H/B), Q 6 (R-FOC-01: 2 X/A; R-FOC-03: 1 C; R-FOC-04: 3
X/Q/A). Scrolling cells (4x4=16): E 5 (R-FOC-01: 1 PaperWM; R-FOC-02:
3 niri/PaperWM/karousel; R-FOC-03: 1 karousel), P 2 (R-FOC-01
niri/karousel), T 4 (R-FOC-01 paneru; R-FOC-02 paneru; R-FOC-03
PaperWM; R-FOC-04 paneru), Q 5 (R-FOC-03 niri/paneru; R-FOC-04
niri/PaperWM/karousel). Ours cells (4x2=8): E 4 (R-FOC-01 Engine
desired-focus plus both adapters' delivery: 2; R-FOC-02 Edge retain:
2), Q 4 (R-FOC-03/04 no-cycle/no-container-focus: 4). Grand
totals: E 27, P 6, T 8, Q 15, M 0 (56 cells). B moves 15 to 16 for
R-FOC-02 stay; W moves 3 to 6 for tie-MRU, cycle-order and container-scope
sub-legs. Classifications are per predicate, not disjoint scenario counts.
The matrix total is now 68 rows.

Ours-vs-consensus position (no behavior selected): R-FOC-02 stay is
strong consensus (B) with Ours matching on both platforms - aligned, no
decision needed. One Ours mismatch stands: R-FOC-01 Ours selects A in
both runs (deterministic first-child descent, fully sourced incl.
delivery) while bspwm/i3/sway select B-then-A (MRU) and PaperWM follows
the topmost member - a stable-vs-MRU choice the user must make if focus
behavior is ever specified. R-FOC-03/04 Ours has no cycle or
container-focus verbs while i3/sway/Q/COSMIC/K carry complete legs -
an inventory gap the user must accept or fill; missing verbs were never
counted as agreeing rejection. No product choice is selected by these
source findings.

## Move expansion (piece B3): R-MOV-06..08 plus R-MOV-01..05 scrolling backfill and explicit-swap legs

Scope: piece B3 adds three GWT move scenarios (R-MOV-06..08), additive
scrolling backfill blocks for R-MOV-01..05, and two explicit-swap fresh
legs reusing R-MOV-01/03. Historical tables and the 58-row audit above
are preserved unchanged. Denominator, families, and the strength rule
are unchanged: consensus classification below counts the original eight
profiles only. The four scrolling profiles form one correlated lineage
reported as an explicit separate non-voting comparison. Cell classes
are mutually exclusive per cell: E complete outcome evidenced; P one
sub-leg evidenced with the remainder TBD; T TBD-only; Q all legs
qualified (fixture-inapplicable / no-counterpart with pinned inventory
evidence); M mixed. E cells carry no TBD; every P cell names its
explicit remainder. Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-MOV-06 nested entry | enter V (index TBD) vs geometric leaf swap vs column carry | enter: I, S; leaf-swap: B (C by MRU), A (B in tile projection); carry: Q (column projection) | C audit-only (three-way conflict; leaf partners differ) |
| R-MOV-07 orthogonal escape | escape H to outer V vs geometric down-swap | escape: I, S; down-swap: B, A (tile projection) | C audit-only (2-2 conflict) |
| R-MOV-08 vertical cross-output | cross to U vs stay local | cross: H, B, I, S, A (5/8, 3 fam); stay: Q; TBD: C, X | A strong cross-family; Ours KDE/Windows stay |

Scrolling comparison (non-voting): R-MOV-06 niri column reorder and
karousel single-window join are complete, PaperWM directional inventory TBD, paneru
peer TBD. R-MOV-07 has no faithful column Given; all four scrolling
profiles are fixture-inapplicable. R-MOV-08 niri stays (in-column edge),
karousel is single-screen inapplicable, PaperWM/paneru TBD. Backfill:
R-MOV-01 edge stay (niri/karousel E); R-MOV-02 in-column swap (niri/
karousel/paneru E); R-MOV-03 reorder/join/swap (niri/karousel/paneru
E); R-MOV-04 all four fixture-inapplicable (no nested H ancestor);
R-MOV-05 edge stay (niri/karousel E). Explicit-swap legs: bspwm no-swap
(R-MOV-01) and east swap (R-MOV-03), i3/sway targeted exchange, and
xmonad/awesome projection misses are complete per leg; qtile has an
internal drag helper but no exposed swap command; niri/karousel/Ours
also have no standalone swap counterpart; rest TBD.

Counts (mutually exclusive E/P/T/Q/M). New rows (3x14=42): E 21, P 3,
T 10, Q 8, M 0. Original-eight new cells (3x8=24): E 12 (R-MOV-06:
B/A; R-MOV-07: B/I/S/A; R-MOV-08: H/B/I/S/Q/A), P 3 (R-MOV-06: I/S/Q),
T 6, Q 3 (R-MOV-06/07: X; R-MOV-07: Q). Scrolling new cells (3x4=12):
E 3 (R-MOV-06: niri/karousel; R-MOV-08: niri), P 0, T 4, Q 5.
Ours new cells (3x2=6): E 6. Backfill (5x4=20): E 10, P 0, T 6,
Q 4. Explicit-swap legs (2x14=28): E 10, P 0, T 8, Q 10.
A grows from 11 to 12; B stays 16; W stays 6. The matrix
total is now 71 rows (68 + 3 new; backfill and swap legs reuse IDs).

Ours-vs-consensus position (no behavior selected): R-MOV-06 Ours midpoint
insert (`V[B,A*,C]`) shares the enter-V leg with i3/sway, whose exact
index remains TBD; it differs from leaf-swap/carry. R-MOV-07 Ours wrap
(`V[V[A,B*],C]`) differs from escape and swap, with no strong consensus.
R-MOV-08 Ours stays with qtile against strong cross-5 (H/B/I/S/A), under
the already-selected V-R4-DIR Up/Down exclusion. The user must decide
whether to retain that exclusion or revise vertical output fallback;
recommend revising it to cross after local movement is exhausted.
Ours has no standalone swap verb; exchange exists only as R2a inside
directional moves. No product behavior is changed by this assessment.

## Resize expansion (piece B4): R-RSZ-01..04 plus R-MIN-01..03 scrolling backfill

Scope: piece B4 adds four GWT resize scenarios (R-RSZ-01..04), each with
14 Then profiles, and additive scrolling backfill blocks for R-MIN-01..03
(original wide tables preserved). Historical tables and the 58-row audit
above are preserved unchanged. Denominator, families, and the strength
rule are unchanged: consensus classification below counts the original
eight profiles only. The four scrolling profiles form one correlated
lineage reported as an explicit separate non-voting comparison. Cell
classes are mutually exclusive per cell: E complete outcome evidenced; P
one sub-leg evidenced with the remainder TBD; T TBD-only; Q all legs
qualified (fixture-inapplicable / no-counterpart with pinned inventory
evidence); M mixed. E cells carry no TBD; every P cell names its explicit
remainder. Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-RSZ-01 keyboard grow/shrink | explicit pixel path vs ratio/weight delta; neighbor allocation; reversibility | pixel: C,H,B,I,S (H/B partial on neighbor/reversal); ratio: X,A; weight: Q | B pixel-path 5/8, 3 families; neighbor/reversal remain audit-only; bare i3/sway defaults use ppt |
| R-RSZ-02 outward edge | no-op vs redistribution | no-op: C, I, S; redistribution: Q; qualified edge-verb absence: X, A | W no-op 3/4ev, 2 families |
| R-RSZ-03 nested scope | nearest split vs ancestor redistribution | nearest inner split: C, I, S | W nearest 3/3ev, 2 families |
| R-RSZ-04 equalize/balance | equal shares vs balance vs missing verb | `-E` .5/.25/.25 plus `-B` thirds: B; normalize thirds: Q | C audit-only (mechanisms and resulting shares differ; no agreement) |

Scrolling comparison (non-voting): R-RSZ-01 niri forward/back preset
cycle is complete (independent columns, no neighbor share);
PaperWM/karousel/paneru stay partial (grid-snap, contextual step plus
viewport recenter, and preset-cycle neighbor mapping TBD). R-RSZ-02 all
four scrolling profiles have no edge-targeted counterpart (pinned action
inventories list no edge verb). R-RSZ-03 all four fixture-inapplicable
(no nested H ancestor). R-RSZ-04 karousel equalize is complete on the
all-visible Given (visible scope is the full strip); paneru is complete
(`Equalize` no-ops on `Single`-column widths, `Balance` sets all columns
to the focused A width 0.5W); niri/PaperWM TBD. Minimum backfill:
karousel min-clamp partial on all three rows; niri partial on R-MIN-01
only; PaperWM/paneru TBD on all three; column Givens now carry the exact
wide-row geometry with full-rect prerequisites, and strip scrolling (not
tree infeasibility) is the explicit context for every remainder.

Counts (mutually exclusive E/P/T/Q/M). New rows (4x14=56): E 21, P 5,
T 12, Q 18, M 0. Original-eight new cells (4x8=32): E 15 (R-RSZ-01: 6
C/I/X/S/A/Q; R-RSZ-02: 4 C/I/S/Q; R-RSZ-03: 3 C/I/S; R-RSZ-04: 2 B/Q),
P 2 (R-RSZ-01: H/B), T 10 (R-RSZ-02: 2 H/B; R-RSZ-03: 2 H/B; R-RSZ-04: 6
C/H/I/X/S/A), Q 5 (R-RSZ-02: 2 X/A; R-RSZ-03: 3 X/Q/A). Scrolling new
cells (4x4=16): E 3 (R-RSZ-01: niri; R-RSZ-04: karousel/paneru), P 3
(R-RSZ-01: PaperWM/karousel/paneru), T 2 (R-RSZ-04: niri/PaperWM), Q 8
(R-RSZ-02: 4 edge-verb absence; R-RSZ-03: 4 fixture-inapplicable). Ours
new cells (4x2=8): E 3 (R-RSZ-01/02/03 KDE via Engine plus adapter
dispatch), Q 5 (Windows keyboard legs have no trigger on 01/02/03;
R-RSZ-04 no equalize counterpart on either platform). Backfill (3x4=12):
P 4 (R-MIN-01: niri/karousel; R-MIN-02/03: karousel), T 8, Q 0. W stays
at 8 (edge-no-op and nearest-split sub-legs). The matrix total is now 75
rows (71 + 4 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): R-RSZ-02 KDE no-op
matches the weak no-op leg (C/I/S); R-RSZ-03 KDE nearest-split matches
the weak nearest leg (C/I/S); R-RSZ-01 KDE matches the strong explicit
pixel-path sub-leg (C/H/B/I/S), not a claim about shipped keybinding units.
Table B grows to 17; the historical C table stays 15 (no historical row
was removed). Two Ours inventory gaps stand:
Windows keyboard resize has no trigger on any of R-RSZ-01/02/03 (rebinds
refuse; pointer resizing is the separate R-MOU-02 path), and neither
platform has an equalize verb for R-RSZ-04 while bspwm/qtile/karousel/
paneru carry complete legs. Windows' absent keyboard trigger is a
strong-leg inventory gap, not a rejecting-policy mismatch. Recommendation
for batch user review: implement Windows keyboard-resize parity using the
shared Engine pixel path; no equalize choice is supported by strong
consensus yet. Missing triggers/verbs never count as agreeing rejection.
No product behavior is changed by this assessment.

## Layout expansion (piece B5): R-LAY-01..04 plus R-FLT-04/R-WS-06 scrolling backfill

Scope: piece B5 adds four GWT layout-command scenarios (R-LAY-01..04),
each with 14 Then profiles, and additive scrolling backfill blocks for
R-FLT-04 (floating area) and R-WS-06 (workspaces area); the layout area
had no existing rows, so there is no other scrolling backfill (0 other
rows, 0 cells). R-FLT-04/R-WS-06 reuse keeps tile/float mode in the
original files: proposed layout selection is not that toggle, and no
historical row is rewritten. Historical tables and the 58-row audit
above are preserved unchanged. Denominator, families, and the strength
rule are unchanged: consensus classification below counts the original
eight profiles only. The four scrolling profiles form one correlated
lineage reported as an explicit separate non-voting comparison. Cell
classes are mutually exclusive per cell: E complete outcome evidenced; P
one sub-leg evidenced with the remainder TBD; T TBD-only; Q all legs
qualified (fixture-inapplicable / no-counterpart with pinned inventory
evidence); M mixed. E cells carry no TBD; every P cell names its
explicit remainder. Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-LAY-01 orientation toggle | same children on the new axis vs geometry-reset no-op | flip: C (`ToggleOrientation`, `S-cos-orient`), B (`node -y`, `S-bsp-type`), I/S (`layout toggle split`, `S-i3-layout` / `S-sway-layout`); no-op: H (immediate geometry recalculation overrides the toggled bit at shipped defaults, `S-hyp-lay` + `S-hyp-defaults`); qualified: X (fixed Tall), Q (`toggle_split` is a different concept), A (fixed tile geometry) | A axis-flip 4/5ev, 3 families (all-but-one evidenced; Ours has no orientation verb on either platform) |
| R-LAY-02 rotate/mirror | 90-degree rotate transform; left/right mirror transform | rotate: B (`-R 90` on the root, `S-bsp-rot`); mirror: B (`-F horizontal` on the root, `S-bsp-rot`); qualified: C/I/X/S (no rotate/mirror verb), H (verbs are parent-scoped only, no whole-fixture transform), Q/A (no nested counterpart) | C audit-only (1/8ev, binary family only) |
| R-LAY-03 master promote | master identity change vs tree operation vs no master concept | promote: X (`swapMaster`, `S-xmo-master`), A (`setmaster`, `S-awe-master`); tree operation: H (`movetoroot`, `S-hyp-lay`); qualified: C/B/I/S/Q (no master concept) | C audit-only (2/8ev, layout-driven family only) |
| R-LAY-04 layout select | per-workspace-scope selection with order preserved | per-workspace scope: B (per-desktop tiled/monocle, `S-bsp-desklay`), I/S (per-parent `layout tabbed`, single-parent fixture, `S-i3-layout` / `S-sway-layout`), X (per-workspace Tall/Mirror Tall, `S-xmo-wslay`), Q (per-group Columns/Max, `S-qti-wslay`), A (per-tag tile/tile.left, `S-awe-tileleft`); partial: H (L2 master rule override, order TBD, `S-hyp-layout`); qualified: C (global config only) | A per-workspace scope 6/8, 3 families (mechanisms differ: desktop/parent/workspace/group/tag ownership; Ours has no select verb on either platform) |

Scrolling comparison (non-voting): R-LAY-01 all four no-counterpart
(no split-axis/orientation verb: `S-nir-acts`, `S-pap-acts`,
`S-kar-acts`, `S-pan-cmds`). R-LAY-02 all four fixture-inapplicable (no
nested H/V counterpart: `S-nir-move`, `S-pap-move`, `S-kar-move`,
`S-pan-model`). R-LAY-03 all four no-counterpart (no master verb, same
four inventories). R-LAY-04 all four no-counterpart (no layout-select
verb; paneru `Virtual*` switches strips, not layouts). R-FLT-04 backfill
all four no-counterpart (no workspace toggle verb; niri/karousel floats
are per-window only, PaperWM scratch is overlay-only, paneru `Manage` is
per-window with a focus-only tier flip). R-WS-06 backfill all four
fixture-inapplicable (no workspace floating mode exists to construct the
`WS2 floating` target with; same four inventories as R-FLT-04, never an
ordinary transfer or a live test on an impossible target).

Counts (mutually exclusive E/P/T/Q/M). New rows (4x14=56): E 15, P 1,
T 0, Q 40, M 0. Original-eight new cells (4x8=32): E 15 (R-LAY-01: 5
C/H/B/I/S; R-LAY-02: 1 B; R-LAY-03: 3 H/X/A; R-LAY-04: 6 B/I/S/X/Q/A), P 1
(R-LAY-04: H rule-override order), T 0,
Q 16 (R-LAY-01: 3 X/Q/A; R-LAY-02: 7 C/H/I/X/S/Q/A; R-LAY-03: 5
C/B/I/S/Q; R-LAY-04: 1 C). Scrolling new cells (4x4=16): Q 16. Ours new
cells (4x2=8): Q 8 (no layout/orient/rotate/mirror/master/select verb in
any Engine/adapter layer on either platform). Backfill (2x4=8): Q 8
(R-FLT-04 and R-WS-06 all four each). Table A grows from 12 to 14;
Table B stays 17; Table W stays 8. The matrix total is now 79 rows
(75 + 4 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): two strong-consensus
gaps stand on both Ours platforms (inventory-evidenced missing verbs,
never counted as agreeing rejection). Propose for batch user review, no
selection or code change: add a parent-axis toggle (R-LAY-01: C/B/I/S
flip the parent to the new axis with the same children) and a
workspace-local layout selection (R-LAY-04: B/I/S/X/Q/A select an
alternative layout at per-workspace scope with order preserved, noting
ownership differs per profile: desktop/parent/workspace/group/tag).
No product behavior is changed by this assessment.

## Workspace expansion (piece B6): R-WS-08..14 plus R-WS-01..05/07 scrolling backfill

Scope: piece B6 adds seven GWT workspace scenarios (R-WS-08..14), each
with 14 Then profiles, and additive scrolling backfill blocks for
R-WS-01..05 and R-WS-07 (R-WS-06 backfill landed earlier; original wide
tables preserved). Historical tables and the 58-row audit above are
preserved unchanged. Denominator, families, and the strength rule are
unchanged: consensus classification below counts the original eight
profiles only. The four scrolling profiles form one correlated lineage
reported as an explicit separate non-voting comparison. Cell classes
are mutually exclusive per cell: E complete outcome evidenced; P one
sub-leg evidenced with the remainder TBD; T TBD-only; Q all legs
qualified (fixture-inapplicable / no-counterpart / owner-specific with
pinned inventory evidence); M mixed qualified and applicable-TBD legs.
E cells carry no TBD; every P/M cell names its explicit remainder.
Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-WS-08 back-and-forth | single previous-view toggle at baseline | toggle: B (`last`), I/S (previous name), Q (previous group), A (previous set) = 5/8, 3 families; variant-gated toggle: H (`=1` toggles 3-2-3, shipped default off only re-activates and never votes as baseline); traversal: PaperWM MRU-stack walk (scrolling, non-voting); qualified: C/X (no verb) | B toggle 5/8 (Ours has no verb on either platform) |
| R-WS-09 return focus | remembered-window focus on switch-back | remembered: C,B,I,X,S,Q,A = 7/8, 4/4 fam; pointer-dependent partial: H (`follow_mouse=1` pointer-hit wins, `=0` restores via `getFocusCandidate`) | B remembered 7/8 (Windows matches; KDE shell-driven leg unresolved) |
| R-WS-10 empty middle | retained vs removed | removed: C,I,S; retained: B,X,Q,A; TBD: H (stable IDs, destruction untraced) | C audit-only (3-4 split) |
| R-WS-11 relative switch | edge-leg wrap (primary mid-inventory legs never vote) | edge wrap: C (default on), B,I,S,Q,A = 6/8, 4/4 fam; primary mid-inventory: C/niri land on existing trailing-empty, Q/A on group/tag 4, B/I/S wrap direct; no-wrap: H (next-creates/prev-history), niri clamp, PaperWM stop, paneru South-creates/North-stops; qualified: X | B edge-wrap 6/8 (Ours has no verb on either platform) |
| R-WS-12 whole-workspace move | semantic whole-domain reassignment | reassign: H,B,I (matched-window form), Q/A (view-ownership verbs) = 5/8, 3/4 fam; destination/focus remain partial; mixed active-only legs: C,S; no-counterpart: X; scrolling niri hidden ByRef sourced, PaperWM hidden applicability TBD (non-voting) | A reassignment 5/8 (Ours has no verb; mechanism differences disclosed) |
| R-WS-13 absent select | create vs refuse | create: H,I,S; refuse: C,B; no-op: X; clamp: niri; fixture-inapplicable: Q/A (static 1-9 cannot construct absent WS9); Ours Win refuses, KDE has no select verb | C audit-only |
| R-WS-14 relative send | relative-target verb exists; follow sub-leg | verb: C,H,B,I,S = 5/8, 3 families; follow splits (C/H/B follow-capable vs I/S no-follow); qualified: X/Q; TBD: A | B verb 5/8 (Ours has no verb on either platform) |

Scrolling comparison (non-voting): R-WS-08 niri single-previous toggle
is complete; PaperWM walks the MRU stack with wrap (traversal, not a
two-state toggle); karousel/paneru have no counterpart. R-WS-09 niri
and paneru restore remembered focus plus saved viewport/origin;
PaperWM retains the selected window with native restore TBD; karousel
is mixed (owner-specific focus plus viewport TBD); Hyprland is
pointer-dependent (named `follow_mouse=0` variant restores B). R-WS-10
niri removes, paneru retains at shipped defaults, PaperWM/karousel are
owner-specific. R-WS-11 primary legs land mid-inventory (niri trailing
existing; PaperWM adjacent-existing; paneru South steps-or-creates,
default off) while edge legs stop (niri/PaperWM) or saturate-or-create
(paneru North/South); karousel has no counterpart. R-WS-12 niri moves
whole workspaces by reference incl hidden, PaperWM stays TBD on hidden
applicability, paneru has no whole-workspace counterpart, karousel is
single-screen inapplicable. R-WS-13 niri clamps, paneru creates,
PaperWM/karousel are owner-specific, Q/A are fixture-inapplicable on
static inventories. R-WS-14 niri/paneru resolve relatively with
edge-stop/focus policies; PaperWM completion follows with insertion
position still partial, and karousel follow remains partial.
Backfill: R-WS-01 niri/PaperWM complete with named
`focus=true` default (niri) and shipped-completion follow (PaperWM;
insertion no-steal is a different journey), karousel/paneru
stay partial on focus/position; R-WS-02 runs the full A-then-B
C-inclusive sequence with A-anchored returns and viewport/focus
remainders on all four; R-WS-03 has no trailing-shortcut counterpart
on any of the four; R-WS-05 float retention is complete on niri
(`focus=true` default follows, `false` stays) and TBD elsewhere;
R-WS-07 listing is owner-specific except niri's cross-workspace MRU
(partial).

Counts (mutually exclusive E/P/T/Q/M). New rows (7x14=98): E 55, P 11,
T 1, Q 27, M 4. Original-eight new cells (7x8=56): E 38 (R-WS-08: 6;
R-WS-09: 7; R-WS-10: 7; R-WS-11: 7; R-WS-12: 0; R-WS-13: 6; R-WS-14: 5),
P 7 (R-WS-09: 1 H; R-WS-10: 1 H; R-WS-12: 5), T 1 (R-WS-14: A), Q 8 (R-WS-08: 2 C/X;
R-WS-11: 1 X; R-WS-12: 1 X; R-WS-13: 2 Q/A; R-WS-14: 2 X/Q), M 2
(R-WS-12: C, S). Scrolling new cells (7x4=28): E 14, P 4, T 0, Q 9,
M 1. Ours new cells (7x2=14): E 3 (R-WS-09 Windows; R-WS-10 Windows;
R-WS-13 Windows), P 0, Q 10, M 1 (R-WS-09 KDE). Table A grows from 14
to 18; Table B grows from 17 to 18; Table U keeps 6 full rows plus the
R-WS-09 KDE return-focus leg; Table C grows from 15 to 17; Table W
stays 8. The matrix total is now 86 rows (79 + 7 new; backfill reuses
IDs). Backfill cells (6x4=24): E 3, P 11, T 3, Q 7.

Ours-vs-consensus position (no behavior selected): four strong-consensus
predicates stand against Ours inventory gaps on both platforms, plus one
unresolved return-focus leg. Recommend for batch user review, no selection
or code change: R-WS-08 add a previous-workspace toggle (B/I/S/Q/A toggle
to the last-viewed workspace at baseline; H toggles only under its
default-off variant; PaperWM traverses MRU instead); R-WS-11 add
relative next/previous workspace switching scored on the edge leg
(C default, B/I/S/Q/A wrap; H creates-or-history instead); R-WS-12
add whole-workspace output reassignment (H/B/I/Q/A agree on the
transfer sub-leg, with destination/focus remainders still queued); R-WS-14
add relative next/previous send (C/H/B/I/S resolve relatively with
split follow policies); R-WS-09 keep the Windows remembered-focus
select path and decide the KDE shell-driven return (U leg: Engine
keeps `last_active`, native focus unestablished). R-WS-10/13
carry no strong consensus after the mechanism/inventory corrections,
so no conflict is established there. No product behavior is changed
by this assessment.

## Minimize expansion: R-MNZ-01..03 (no scrolling backfill)

Scope: piece B7 adds three GWT minimize scenarios (R-MNZ-01..03), each
with 14 Then profiles. The minimize area had no existing rows, so there
is no scrolling backfill to assess (0 existing rows, 0 cells).
R-MIN-01..03 size-constraint rows are reused by citation only, never
duplicated: clamp policy there proves no minimize path. Historical
tables and the 58-row audit above are preserved unchanged. Denominator,
families, and the strength rule are unchanged: consensus classification
below counts the original eight profiles only. The four scrolling
profiles form one correlated lineage reported as an explicit separate
non-voting comparison. A hide verb alone never votes: only a sourced
client/host iconify request path (or its evidenced refusal/absence)
counts here; scratchpad, vacant flags, ordinary float state, and sticky
never substitute. Cell classes are mutually exclusive per cell: E
complete outcome evidenced; P one sub-leg evidenced with the remainder
TBD; T TBD-only (request consumed or untraced with unknown tree effect);
Q evidenced request refusal or protocol no-op with inventory evidence; M
mixed. E cells carry no TBD; every P cell names its explicit remainder.
Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-MNZ-01 middle minimize | minimized window leaves tiling allocation with a restore path | leaves: C (tiling unmap into restore state, `S-cos-minimize`), Q (hide as `MINIMIZED` float, `S-qti-minimize`), A (ban from arrangement, `S-awe-minimize`); non-voting: B/I (no counterpart/refusal), H/S/X (TBD tree effect) | W leaves-allocation 3/3ev, 2 families |
| R-MNZ-02 restore | restore returns B to its old slot | old slot: C (stored tiling-state remap, `S-cos-minimize`), A (remap at retained client order, `S-awe-minimize`); path-only: Q (`S-qti-minimize`); non-voting: B/I, H/S/X | W old-slot 2/2ev thin, 2 families |
| R-MNZ-03 sole minimize | minimized occupancy vs workspace cleanup; focus fallback | occupancy stored: C (`minimized_windows`, `S-cos-minimize`); hide paths: Q/A (`S-qti-minimize`/`S-awe-minimize`); non-voting: B/I, H/S/X; all cleanup/focus legs TBD | C audit-only |

Scrolling comparison (non-voting): R-MNZ-01 PaperWM scratch-layer move,
karousel `TiledMinimized` transition, and paneru `Minimized` mark are
partial (reflow/focus TBD); niri is no-counterpart (protocol no-op plus
no Action verb). R-MNZ-02 PaperWM `unmakeScratch`, karousel
minimizedChanged retile, and paneru mark removal are partial
(position/focus TBD); niri stays no-counterpart. R-MNZ-03 PaperWM,
karousel, and paneru stay TBD on sole-occupancy/cleanup; niri stays
no-counterpart. i3's rejection and niri's no-op are qualified
no-counterparts with request-path evidence, never agreeing no-ops.

Counts (mutually exclusive E/P/T/Q/M). New rows (3x14=42): E 2, P 16,
T 15, Q 9, M 0. Original-eight new cells (3x8=24): E 2 (R-MNZ-01 C/A),
P 7 (R-MNZ-01 Q; R-MNZ-02 C/Q/A; R-MNZ-03 C/Q/A), T 9 (H/S/X on all
three rows), Q 6 (B/I on all three rows). Scrolling new cells (3x4=12):
E 0, P 6, T 3, Q 3, M 0. Ours new cells (3x2=6): P 3 (Windows slot legs,
focus TBD) and T 3 (KDE native/Engine journey). Table A stays 18;
Table B stays 18; Table U stays 6 full
rows plus the R-WS-09 KDE leg; Table C stays 17; Table W grows from 8
to 10. The matrix total is now 89 rows (86 + 3 new; no backfill reuses
IDs).

Ours-vs-consensus position (no behavior selected): no strong consensus
exists on any minimize predicate (two weak legs plus one audit-only
row), so no strong Ours conflict is established. Weak differences are
recorded accurately: R-MNZ-01 weak direction (C/Q/A) leaves Ours KDE
fully TBD (frame-read branch) while Ours Windows retains its slot
against the weak direction; R-MNZ-02 weak old-slot (C/A thin) matches
Ours Windows (kept slot, focus TBD) with Ours KDE TBD. No selection,
no code change, and no user decision is required by these findings.
No product behavior is changed by this assessment.

## Maximize expansion (piece B8): R-MAX-08..09 plus R-MAX-01..07 scrolling backfill

Scope: piece B8 adds two GWT maximize/fullscreen scenarios (R-MAX-08
focus/move while maximized, R-MAX-09 send while maximized/fullscreen),
each with 14 Then profiles, and additive scrolling backfill blocks for
R-MAX-01..07 (original wide tables preserved). Historical tables and
the 58-row audit above are preserved unchanged. Denominator, families,
and the strength rule are unchanged: consensus classification below
counts the original eight profiles only. The four scrolling profiles
form one correlated lineage reported as an explicit separate non-voting
comparison. A missing maximize verb/state never votes: ordinary-manage
or fullscreen-only reports from maximize-stateless profiles are feature
absence, not deliberate overlay rejection. R-MAX-09 splits into a
`B:max` leg (qualifies where no maximize state exists) and a fresh
`B:full` leg (applicable wherever fullscreen exists); M cells carry one
qualified leg plus one applicable-TBD leg. Host-owned zoom is
owner-specific: paneru has no maximize verb/state, and the host-zoom
journey stays TBD (queued), never a no-counterpart. Cell classes are mutually
exclusive per cell: E complete outcome evidenced; P one sub-leg
evidenced with the remainder TBD; T TBD-only; Q all requested legs
qualified (no-counterpart / fixture-inapplicable with pinned inventory
evidence); M mixed (qualified plus applicable-TBD legs, including
owner-specific host journeys). E cells carry no TBD; every P/M cell names its
explicit remainder. Counts measure documentation coverage, not votes.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-MAX-08 focus/move while maximized | overlay focus fence vs sibling access; hidden-tree move, refusal, overlay clearing, actual focused mover | fence by occlusion: A (maximized B at x=0 qualifies no directional target, focus retained; swap hits C with no tile change: `S-awe-focus`+`S-awe-geodir`+`S-awe-swap`); access: Q (column-index step, `S-qti-focus`); qualified max-absence: B, I, X, S; TBD: C, H | C audit-only |
| R-MAX-09 send while maximized/fullscreen | state carried vs restored before transfer; source slot/reflow, target overlay, follow | full-leg carry: B (same vacant node, `S-bsp-xfer`), I (same container re-attached, `S-i3-movews`), S (same container re-added, `S-sway-full`), Q (window property travels, `S-qti-group`), A (boolean property travels, `S-awe-tag`) = 5/8, 3 families; max leg: Q/A carry, B/I/X/S absent, C/H TBD | A full-carry 5/8 (Ours Windows refuses fullscreen; KDE TBD) |

Scrolling comparison (non-voting): R-MAX-08 PaperWM converts to
width-maximize (no overlay; switch access plus model swap), niri steps
and reorders with the maximized flag untouched, karousel clears the
overlay through the focus change itself, paneru has no paneru-native
maximize preparation (host zoom is an owner-specific journey, TBD).
R-MAX-09 niri window-send strips to Normal (column-send would retain and
is not this leg), PaperWM moves a width-maximized window or a fullscreen
window with carry TBD, karousel transfers the column with native-state
carry TBD, paneru max leg is owner-specific TBD with a TBD
fullscreen-marker carry. Backfill: R-MAX-01 P3/M1 (niri native setter,
PaperWM width conversion (native restore moot), karousel membership plus skip;
paneru host-zoom journey and exact hinted frames/convergence TBD);
R-MAX-02 E3/P1 (niri flag plus plain
activations, PaperWM saved-frame restore, karousel focus-change restore;
paneru exit TBD); R-MAX-03 all four fixture-inapplicable (no workspace
floating mode); R-MAX-04 E3/M1 (native toggles/handlers without fences;
paneru host-zoom journey TBD); R-MAX-05 E2/Q2 (niri/PaperWM no-refusal
and retained-column/saved-frame restore sourced; karousel/paneru have no project
toggle verb); R-MAX-06 E1/P2/M1 (niri scrolling-layout admission with
focus; PaperWM conversion and karousel force-unmaximize with focus TBD;
paneru host-zoom journey TBD); R-MAX-07 E4 (flag-only overlay entry
everywhere; size never infers).

Counts (mutually exclusive E/P/T/Q/M). New rows (2x14=28): E 12, P 6,
T 3, Q 4, M 3. Original-eight new cells (2x8=16): E 6 (R-MAX-08: A;
R-MAX-09: B/I/S/Q/A), P 3 (R-MAX-08: Q; R-MAX-09: C/H), T 2 (R-MAX-08:
C/H), Q 4 (R-MAX-08: B/I/X/S), M 1 (R-MAX-09: X max-absent plus
applicable full).
Scrolling new cells (2x4=8): E 3 (both niri plus R-MAX-08 PaperWM),
P 3 (R-MAX-08 karousel plus R-MAX-09 PaperWM/karousel), M 2 (both paneru
host-zoom journeys).
Ours new cells (2x2=4): E 3 (R-MAX-08 both; R-MAX-09 Windows), T 1
(R-MAX-09 KDE same-output delivery plus carry TBD). Backfill (7x4=28):
E 13, P 6, Q 6, M 3, T 0. Total 56 cells: E25/P12/T3/Q10/M6.
Table A grows from 18
to 19 (R-MAX-09 full-carry); Table B stays 18; Table U stays 6 full rows
plus the R-WS-09 KDE leg; Table C stays 17; Table W stays 10. The matrix
total is now 91 rows (89 + 2 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands against Ours. R-MAX-09 full-carry is strong 5/8
(B/I/S/Q/A across binary, tree, and layout-driven families): a
fullscreen window carries its state to the target workspace rather than
restoring first. Ours Windows refuses the fullscreen send
(`send-refused-fullscreen`, no writes) while carrying the maximized
send; Ours KDE's same-output native-send outcome remains TBD (the
cited project delivery path covers R4 cross-output sends; host moves
reach the desktops observer). Recommend for batch user
review, no selection or code change: carry fullscreen state across
workspace send (B/I/S/Q/A move the same container/node/client with its
mode flag; niri window-send strips instead and is the disclosed
counter-model). Resolve KDE's native-send outcome before proposing a
behavior change there.
R-MAX-08 carries no strong consensus (fence-by-occlusion 1, access 1): Ours focus-exempt plus R2c wrap is recorded without a
conflict. No product behavior is changed by this assessment.

## Groups expansion (piece B9): R-GRP-02..03 plus R-GRP-01 scrolling backfill

Scope: piece B9 adds two GWT groups scenarios (R-GRP-02 sequential join
then leave, R-GRP-03 close active tab), each with 14 Then profiles, and
an additive scrolling backfill block for R-GRP-01 (original wide table
preserved). Historical tables and the 58-row audit above are preserved
unchanged. Denominator, families, and the strength rule are unchanged:
consensus classification below counts the original eight profiles only.
The four scrolling profiles form one correlated lineage reported as an
explicit separate non-voting comparison. Cell classes are mutually
exclusive per cell: E complete outcome evidenced with no TBD; P one
sub-leg evidenced with the remainder TBD and queued; T TBD-only
(verb inventory alone never promotes to P); Q all legs qualified
(fixture-inapplicable / no-counterpart with pinned inventory evidence);
M mixed. Counts measure documentation coverage, not votes. Semantic
tab-bar membership suffices; pixel render never gates a semantic vote.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-GRP-02 sequential join then leave | join B left into `S[A,C]` vs swap; one right operation from the actual post-join B as a new tile vs reorder vs dissolve | join: H (`moveIntoGroup` after-current plus newcomer current/focus, `S-hyp-grpmove`); leave: H (`moveOutOfGroup` with shipped focus_removed_window, `S-hyp-grpmove`); conditional: C (in-stack Right reorder vs edge `MoveOut` on the actual post-join index, join TBD, `S-cos-grpmove`) | C audit-only (1/8ev join, 1/8ev leave; no agreement) |
| R-GRP-03 close the active tab | group retained vs flattening; focus C (neighbor and MRU coincide) | retained: C (2-tab stack via active-index clamp, `S-cos-grpclose`), H (group retained via remove path, `S-hyp-close` + `S-hyp-grpmove`), I (parent retained, `S-i3-grp` + `S-i3-close`), S (parent retained, `S-sway-close` + `S-sway-layout`); focus C: C (active tab C), H (grouped next), I (`con_next_focused` focus-stack next C), S (focus-inactive MRU C) | A retained 4/4ev plus focus-C 4/4ev, 3 families (COSMIC n-ary, Hyprland binary, i3/sway tree) |

Scrolling comparison (non-voting): R-GRP-02 niri joins appended last as
`[A,C,B*]` with B activated then expels to a new sole column
(`S-nir-consume`); karousel joins appended last with focus-taker B then
leaves to a new own column (`S-kar-grpmove`); PaperWM `slurp(A)` from the
A-focused variant joins B last with selection TBD, then `barf` expels B
(shipped RIGHT `slurp(B)` is a no-op with no right neighbor,
`S-pap-slurp`); paneru `Swap` peer untraced. R-GRP-03 all four scrolling
profiles TBD on member-removal fixup. Backfill: R-GRP-01 niri toggles
Normal/Tabbed plus `focus_up`/`focus_down` member step
(`S-nir-consume`); karousel toggles stacked plus `focusUp`/`focusDown`
member step (`S-kar-grpmove`); PaperWM has no tabbed/stacked toggle
counterpart; paneru exact `S` fixture is inapplicable per
`S-pan-model` (native-tab variant under R-COL-10).

Counts (mutually exclusive E/P/T/Q/M). New rows (2x14=28): E 7
(R-GRP-02: H/niri/karousel; R-GRP-03: C/H/I/S), P 2 (R-GRP-02:
C/PaperWM), T 7 (R-GRP-02: I/S/paneru; R-GRP-03:
niri/PaperWM/karousel/paneru), Q 12 (B/X/Q/A plus both Ours each row),
M 0. Original-eight new cells (2x8=16): E 5 (R-GRP-02: H; R-GRP-03:
C/H/I/S), P 1 (R-GRP-02: C conditional), Q 8 (B/X/Q/A each row), T 2
(R-GRP-02: I/S), M 0. Scrolling new cells (2x4=8): E 2 (R-GRP-02
niri/karousel), P 1 (R-GRP-02 PaperWM), T 5 (R-GRP-02 paneru plus all
four R-GRP-03). Ours new cells (2x2=4): Q 4 (no tab carrier and no
join/leave verb, `S-ours-grp`). Backfill (1x4=4): E 2 (niri/karousel
toggle plus step), Q 2 (PaperWM no-toggle counterpart, paneru
fixture-inapplicable). Total 32 cells: E9/P2/T7/Q14/M0. Table A grows 19
to 20 (R-GRP-03 retained plus focus-C); Table B stays 18; Table U stays 6
full rows plus the R-WS-09 KDE leg; Table C stays 17; Table W stays 10.
The matrix total is now 93 rows (91 + 2 new; backfill reuses R-GRP-01).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands against Ours. R-GRP-03 retained 4/4ev plus focus-C 4/4ev
(C/H/I/S across n-ary, binary, and tree families): a closed active tab
leaves a retained 2-tab group focused on C. Both Ours platforms lack a
tab carrier and any semantic join/leave/close-tab verb (`S-ours-grp`:
split-only `Node` plus operation-family inventory plus fail-closed
center-stack), so the `S` fixture itself is inapplicable. Propose for
batch user review, no selection or code change: retain the standing
V-GROUP-STACK deferral (refuse closed) while recording the strong foreign
retained-plus-C direction if tabs are ever specified. Missing verbs were
never counted as agreeing rejection. R-GRP-02 carries no strong
consensus (join 1/8ev, leave 1/8ev plus a conditional C leg). No product
behavior is changed by this assessment.

## Floating expansion (piece B10): R-FLT-12..14 plus R-FLT-01..03/05..11 scrolling backfill

Scope: piece B10 adds three GWT floating scenarios (R-FLT-12 raise/lower,
R-FLT-13 ordinary-float workspace switch, R-FLT-14 pointer drag/resize),
each with 14 Then profiles, and additive scrolling backfill blocks for
R-FLT-01..03 and R-FLT-05..11 (R-FLT-04 backfill landed earlier; original
wide tables preserved). Historical tables and the 58-row audit above are
preserved unchanged. Denominator, families, and the strength rule are
unchanged: consensus classification below counts the original eight
profiles only. The four scrolling profiles form one correlated lineage
reported as an explicit separate non-voting comparison. An absent tiler
verb never denies a host pointer journey: free drag/resize stays TBD
(queued), never no-counterpart. Cell classes are mutually exclusive per
cell: E complete outcome evidenced with no TBD; P one sub-leg evidenced
with the remainder TBD and queued; T TBD-only; Q all legs qualified
(no-counterpart / fixture-inapplicable with pinned inventory evidence);
M mixed. E cells carry no TBD; every P cell names its explicit remainder.
Counts measure documentation coverage, not votes. Semantic outcomes lead
each Then in one to three lines; source keys follow.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-FLT-12 raise/lower | an F-raising path exists vs explicit lower | raise: C (focus raise, `S-cos-raise`), H (float-toggle/activate/click raise, `S-hyp-raise`), B (focus restack above, `S-bsp-stack`), I (activate/click to floating-list tail, `S-i3-raise`), S (press/map/move/resize raise, `S-sway-fltraise`), Q (activation bring-to-front, `S-qti-raise`), A (`c:raise`, `S-awe-raise`) = 7/8, 4 fam; layer-only: X (`S-xmo-restack`); lower to layer bottom: Q/A 2/2ev, one family | U raise 7/8 (Ours relative order TBD both); W lower 2/2ev (one-family weak) |
| R-FLT-13 ordinary visibility | ordinary float hidden on switch vs sticky-like visibility | hidden: 8/8, 4 fam (C per-workspace layer, H workspace spaces, B desktop show/hide, I per-workspace list, X refresh member filter, S per-workspace list, Q group hide, A tag membership); frame/focus per profile in the matrix | B hidden 8/8 (Windows matches hide/reveal with last-focus; KDE native journey TBD, listed as a U leg) |
| R-FLT-14 pointer drag/resize | free frame retention vs snap/clamp/tiling | free frames: 8/8, 4 fam (C floating drop/resize grabs, H snap-off writes, B rectangle writes, I drag/resize with raise, X raw writes, S pending writes with raise, Q tweak/resize, A frame writes) | U free 8/8 (both Ours platforms refuse the project route with `NotTiled`; host journeys TBD) |

Scrolling comparison (non-voting): R-FLT-12 niri raises on focus with
no lower verb (`S-nir-fltact`); PaperWM ordinary non-sticky floats
inapplicable and dialog-float relative stacking TBD (`S-pap-float`);
karousel/paneru TBD on the
host journey (`S-kar-float`, `S-pan-flt` semantics). R-FLT-13 niri/paperwm/
karousel/paneru hide per workspace/strip/space/desktop with frame/focus
remainders queued. R-FLT-14 niri is free with Mod producers; PaperWM/
karousel/paneru are host-journey TBD. Backfill: R-FLT-01 niri/karousel/
paneru toggle with position/focus TBD, PaperWM no ordinary subject;
R-FLT-02 niri no sticky subject, PaperWM scratch-stuck, karousel/paneru
host TBD; R-FLT-03 niri/karousel widths stable, PaperWM ordinary float-out
has no counterpart, paneru TBD; R-FLT-05 niri no subject, others TBD;
R-FLT-06 niri/karousel state with interplay TBD, PaperWM ordinary float
toggle has no counterpart, paneru
owner-specific host TBD; R-FLT-07 niri/PaperWM/karousel tile-A with
float-exclusion established, paneru TBD; R-FLT-08/09 niri float search or
retain, karousel retain, PaperWM remembered tiled selection and paneru
traversal TBD; R-FLT-10/11 niri 50px steps,
karousel retained, PaperWM/paneru TBD.

Counts (mutually exclusive E/P/T/Q/M). New rows (3x14=42): E 18, P 10,
T 6, Q 0, M 8. Original-eight new cells (3x8=24): E 17 (R-FLT-12: Q/A;
R-FLT-13: C/B/I/X/S/Q/A; R-FLT-14: C/H/B/I/X/S/Q/A), P 5 (R-FLT-12:
C/H/B/X; R-FLT-13: H), T 0, Q 0, M 2 (R-FLT-12: I/S). Scrolling new
cells (3x4=12): E 1 (R-FLT-14: niri), P 4 (R-FLT-13: all four), T 5
(R-FLT-12: karousel/paneru; R-FLT-14: PaperWM/karousel/paneru), Q 0,
M 2 (R-FLT-12: niri/PaperWM). Ours new cells (3x2=6): P 1 (R-FLT-13
Windows), T 1 (R-FLT-13 KDE), Q 0, M 4 (R-FLT-12 both; R-FLT-14 both).
Backfill (10x4=40): E 13, P 7, T 14, Q 5, M 1. Total 82 cells:
E31/P17/T20/Q5/M9. M includes evidenced raise plus absent lower (I/S/niri),
not only qualified plus TBD; those three cells add no live case.
Table B grows 18 to 19 (R-FLT-13 hidden 8/8);
Table U grows from 6 full rows to 8 (R-FLT-12 raise, R-FLT-14 free)
plus the two KDE legs; Table W grows 10 to 11 (layer-bottom lower);
Tables A/C unchanged. The matrix total is now
96 rows (93 + 3 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): two new strong
predicates stand with Ours delivery-only or refused. R-FLT-12 raise-7/8
(C/H/B/I/S/Q/A across all four original families): every profile can
raise F. Both Ours platforms dispatch focus (`setActive` /
`actuate_focus`) with keepAbove/topmost bands, but relative F/G order is
host stacking and TBD on both; explicit lower has only Q/A one-family
weak agreement. R-FLT-14 free-8/8 (all four families): pointer drag/resize
keeps the free frame. Both Ours platforms refuse the project route
(shared-Engine `NotTiled`); only host KWin/Win32 journeys could match,
both TBD. R-FLT-13 hidden-8/8: Ours Windows matches via hide/reveal with
last-focus return; Ours KDE's native select journey stays TBD.
Recommend for batch user review, no selection or code change: trace the
KDE native-select float journey, the F/G order effect of activation on
both platforms, and the host move/size journeys before proposing any
change. Missing verbs and TBD journeys never count as agreeing
rejection. No product behavior is changed by this assessment.

## Close expansion (piece B11): R-CLOSE-03..05 plus R-CLOSE-01/02 scrolling backfill

Scope: piece B11 adds three GWT close scenarios (R-CLOSE-03 sole close,
R-CLOSE-04 float close, R-CLOSE-05 maximized/fullscreen close), each with
14 Then profiles, and additive scrolling backfill blocks for R-CLOSE-01/02
(original wide tables preserved). Historical tables and the 58-row audit
above are preserved unchanged. Denominator, families, and the strength
rule are unchanged: consensus classification below counts the original
eight profiles only. The four scrolling profiles form one correlated
lineage reported as an explicit separate non-voting comparison. A missing
maximize verb/state never votes: ordinary-manage reports from
maximize-stateless profiles are feature absence, not deliberate overlay
rejection. R-CLOSE-05 splits into a `B:max` leg (qualifies where no
maximize state exists) and a fresh `B:full` leg (applicable wherever
fullscreen exists); M cells carry one qualified leg plus one applicable
leg, whether that applicable leg is TBD (paneru host-zoom, X/S full) or
evidenced (B/I max-absent plus full-E), per the floating-piece
precedent; no bare markers. All three scenarios use the shared
`S(S-close-verbs)` inventory (one named verb per profile, no kill-only
substitution). Cell classes are mutually exclusive per cell: E complete outcome
evidenced with no TBD; P one sub-leg evidenced with the remainder TBD and
queued; T TBD-only; Q all legs qualified with pinned inventory evidence;
M mixed. E cells carry no TBD; every P cell and applicable-TBD M leg names
its explicit remainder. Counts measure documentation coverage, not votes. Semantic
outcomes lead each Then in one to three lines; source keys follow.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-CLOSE-03 sole close | shown workspace retained with focus none | retained: C (active kept, `S-cos-rem`+`S-cos-send`), B (explicit-only removal, `S-bsp-wsretain`), I (visible kept, `S-i3-wsretain`), X (static zipper, `S-xmo-ws`), S (active kept, `S-sway-wsretain`), Q (static groups, `S-qti-wsdef`), A (static tags, `S-awe-hist`) = 7/8, 4/4 fam; focus none: C/I/S/Q/A complete, B/X partial; TBD: H (retention and focus) | U retained 7/8 (Ours Engine collapse sourced, native journey TBD) |
| R-CLOSE-04 float close | tiles untouched; focus B | untouched: C (floats unmap outside the tiling tree, `S-cos-flttoggle`), B (no tiling space, `S-bsp-float`), I (wrapper detach), S (floating list), Q (outside Columns), A (excluded, `S-awe-tile`) = 6/8, 4/4 fam; focus B: same six via MRU/stack/history paths; partial: H/X (removal sourced, exact target TBD) | U untouched 6/8 plus focus-B 6/8 (Ours Engine exception-drop plus preserved focus sourced, adapter journey TBD) |
| R-CLOSE-05 overlay close | no restore; survivor refills with focus A (full leg) | full leg: H (unmap refocus, `S-hyp-close`), Q/A (float-state removal, `S-qti-fs`/`S-awe-fs`), B (promotion plus history guess), I (detach plus focus-stack next) = 5/8, 3 fam; max leg: H/Q/A complete, C partial, B/I/X/S qualified absent; partial: C/PaperWM removal, X/S full-leg TBD | U full-leg 5/8 (Ours Engine removal plus desired focus sourced, overlay-native cleanup TBD) |

Scrolling comparison (non-voting): R-CLOSE-01 niri/PaperWM/karousel
complete (order plus next-column/topmost/left-neighbor focus:
`S-nir-close`, `S-pap-close`, `S-kar-close`); paneru order with
nearest-center focus TBD. R-CLOSE-02 niri complete on widths and
admission (faithful 0.5/0.3/0.2W via `SetColumnWidth`, no tree-ratio
rescale, fresh column after active; reopened focus TBD) and PaperWM
complete (50/30/20 prepared via the `resizeW` 10% grid at zero
gaps/margins, survivor frame-widths stable, fresh at selected+1 with
activate-on-show); karousel preserves host-interactively resized widths
and fresh-admits after last focus, with reopened focus TBD; paneru
prepares exact ratios via `SetWidth` with reopened focus TBD.
R-CLOSE-03 niri/PaperWM/karousel complete (active-spare retention,
selection none, last-column destroy: `S-nir-close`, `S-pap-close`,
`S-kar-close`); paneru partial (row-0 spare exact per the Space1 VW
fixture, focus TBD). R-CLOSE-04 niri complete (float removal
with scrolling untouched plus MRU return); PaperWM/karousel/
paneru partial (separation sourced, shell/KWin/geometry focus TBD).
R-CLOSE-05 niri/karousel complete (flag-agnostic removal with
independent survivor widths, sole-survivor focus, no restore);
PaperWM partial (allocation/focus TBD); paneru mixed (host-zoom
owner-specific plus fullscreen-strip TBD).

Counts (mutually exclusive E/P/T/Q/M). New rows (3x14=42): E 20, P 17,
T 0, Q 0, M 5. Original-eight new cells (3x8=24): E 14 (R-CLOSE-03: 5
C/I/S/Q/A; R-CLOSE-04: 6 C/B/I/S/Q/A; R-CLOSE-05: 3 H/Q/A), P 6
(R-CLOSE-03: 3 H/B/X; R-CLOSE-04: 2 H/X; R-CLOSE-05: 1 C), T 0,
Q 0, M 4 (R-CLOSE-05: B/I/X/S max-absent plus applicable full, full-E
on B/I). Scrolling new cells (3x4=12): E 6 (R-CLOSE-03: niri/PaperWM/
karousel; R-CLOSE-04: niri; R-CLOSE-05:
niri/karousel), P 5, T 0, Q 0, M 1 (R-CLOSE-05 paneru). Ours new cells
(3x2=6): P 6 (Engine collapse and desired focus sourced; adapter native
journeys TBD). Backfill (2x4=8): E 4 (R-CLOSE-01 niri/PaperWM/karousel
plus R-CLOSE-02 PaperWM), P 4 (R-CLOSE-01 paneru plus R-CLOSE-02
niri/karousel/paneru), M 0. Total 50 cells: E24/P21/T0/Q0/M5. Table U grows from
8 to 11 full rows plus the two KDE legs; Tables A/B/C/W unchanged. The
matrix total is now 99 rows (96 + 3 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): three new strong
predicates stand with Ours unresolved, never differing. R-CLOSE-03
retained-7/8 (C/B/I/X/S/Q/A across all four families): the shown
workspace survives closing its sole window. R-CLOSE-04 untouched-6/8
plus focus-B-6/8 (C/B/I/S/Q/A across all four families): floats close
outside the tiling with MRU refocus. R-CLOSE-05
full-leg-5/8 (H/B/I/Q/A across three families): a closed fullscreen
window needs no restore; the survivor refills with focus. Ours Engine
establishes collapse, exception-drop, and desired focus on both
platforms (`S-ours-close`); adapter native journeys (empty-focus,
float-focus, overlay cleanup) stay TBD. Recommend for batch user
review, no selection or code change: establish the three Ours native
close journeys before proposing any behavior change; the strong foreign
direction is recorded for any future close-behavior specification.
Missing triggers and TBD journeys never count as agreeing rejection. No
product behavior is changed by this assessment.

## Multi-output expansion: R-OUT-03..06 plus R-OUT-01/02 scrolling backfill

Scope: this piece adds four GWT multi-output scenarios (R-OUT-03
cross-output focus, R-OUT-04 explicit output send, R-OUT-05 open with
two outputs, R-OUT-06 disconnect/reconnect), each with 14 Then
profiles, and additive scrolling backfill blocks for R-OUT-01/02
(original wide tables preserved). Historical tables and the 58-row
audit above are preserved unchanged. Denominator, families, and the
strength rule are unchanged: consensus classification below counts the
original eight profiles only. The four scrolling profiles form one
correlated lineage reported as an explicit separate non-voting
comparison. karousel two-output fixtures are fixture-inapplicable per
the sourced single-screen scope (`S-kar-single`), never no-op votes.
paneru fixtures keep one strip per display (each display owns its
strip and native Space). Ours cells cite production Engine plus
separate adapter evidence (`S-ours-focus`, `S-ours-move`, `S-ours-ws`
plus the new `S-ours-out`); the Engine workspace send refuses
cross-output, so the R-OUT-04 Ours legs are qualified
no-counterparts, not agreeing rejection. Cell classes are mutually
exclusive per cell: E complete outcome evidenced with no TBD; P one
sub-leg evidenced with the remainder TBD and queued; T TBD-only; Q all
legs qualified with pinned inventory evidence; M mixed. Counts measure
documentation coverage, not votes. Semantic outcomes lead each Then in
one to three lines.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-OUT-03 cross-output focus | crosses to L; target X vs stays | cross: B (west search spans monitors, `S-bsp-flt-focus`+`S-bsp-move-target`), I (workspace-level fallback returns L's visible workspace, `S-i3-outfocus`), X (all-visible-screen candidates, `S-xmo-scope`), S (output fallback after tree walk, `S-sway-focus`), A (global miss then screen cross, `S-awe-focus`), H partial (monitor fallback, target TBD, `S-hyp-focus`) = 6/8, 3 fam; target-X: B/I/X/S/A = 5/8, 3 fam; stay: Q (`left()` in-group only, `S-qti-focus`); fall-through branch TBD: C | B cross-6/8 plus target-X-5/8 (Ours matches) |
| R-OUT-04 explicit send | carry to L; follow vs stay | carry: C (`MoveToOutput`/`SendToOutput`, `S-cos-out`), H (`movetoworkspace`, `S-hyp-movews`), B (`transfer_node`, `S-bsp-send`+`S-bsp-xfer`), I/S (TAIL attach, `S-i3-outmove`/`S-sway-outmove`), X (`W.shift`, `S-xmo-scope`), Q (`togroup`, `S-qti-group`), A (`move_to_tag`, `S-awe-tag`) = 8/8, 4/4 fam; visible follow: C Move/H follow/B `--follow`/I/S/Q follow/A activate = 7/8, 4/4 fam; X stays on source view, target-stack focus sourced, source refocus TBD | A carry-8/8 plus follow-7/8 (Ours has no send counterpart) |
| R-OUT-05 open routing | lands on the focused output with newcomer focus | focused-L plus newcomer focus: C (`S-cos-out`+`S-cos-mapfocus`), B (`S-bsp-insert`), I (`S-i3-admit`), X (`S-xmo-admit`), S (`S-sway-ins`), Q (`S-qti-add`), A (`S-awe-manage`) = 7/8, 4/4 fam; TBD: H | U focused-output-7/8 (Ours routing TBD; HISTORICAL map-only tally, superseded for motion-first legs by the [cursor-first supplement](#cursor-first-output-targeting-supplement-2026-10-20)) |
| R-OUT-06 disconnect | evacuation destination and return | retain monitor/desktops: B (shipped remove-unplugged=false; same-id return reuses monitor, `S-bsp-monrm`); evacuate: S (highest-available else fallback, `S-sway-evac`); TBD: C, H, I, X, Q, A | C audit-only (no agreed predicate) |

Scrolling comparison (non-voting): R-OUT-01 niri/PaperWM/paneru stay
under the semantic directional move (niri `move_left` edge-false,
PaperWM same-space `swap` edge return, paneru `Swap(West)` with no
western peer and no West display fall-through:
`S-nir-move`+`S-nir-mon`, `S-pap-mon`, `S-pan-swap`); karousel
fixture-inapplicable (`S-kar-single`). R-OUT-02 same three stay
(single-column edge under the same verbs); karousel
fixture-inapplicable. R-OUT-03 niri/PaperWM stay (per-workspace/
per-space edge-false: `S-nir-focus`, `S-pap-focus`); paneru traversal
TBD. R-OUT-04 niri carries with follow; PaperWM carries
via `switchMonitor` window carry; paneru appends with Follow focus
(`S-nir-mon`, `S-pap-mon`, `S-pan-display`). R-OUT-05 PaperWM lands on
the selected space with activation (`S-pap-ins`); niri routing,
paneru display routing and Hyprland cursor-vs-active routing TBD.
R-OUT-06 all applicable scrolling profiles TBD (PaperWM GNOME mirror
alone establishes no window outcome); karousel fixture-inapplicable.

Counts (mutually exclusive E/P/T/Q/M). New rows (4x14=56): E 28,
P 7, T 15, Q 6, M 0. Original-eight new cells (4x8=32): E 20
(R-OUT-03: B/I/X/S/Q/A; R-OUT-04: C/H/B/I/S/Q/A; R-OUT-05:
C/B/I/X/S/Q/A), P 5 (R-OUT-03: C/H; R-OUT-04: X; R-OUT-06: S/B),
T 7 (R-OUT-05: H; R-OUT-06: C/H/I/X/Q/A), Q 0. Scrolling new cells
(4x4=16): E 6 (R-OUT-03: niri/PaperWM; R-OUT-04: niri/PaperWM/paneru;
R-OUT-05: PaperWM), P 0, T 6, Q 4 (karousel
x4). Ours new cells (4x2=8): E 2 (R-OUT-03 cross to X), Q 2
(R-OUT-04 same-output-only refusal), P 2 (R-OUT-05 admission
anchor), T 2 (R-OUT-06 journeys). Backfill (2x4=8): E 6
(R-OUT-01/02 niri/PaperWM/paneru stay), Q 2 (karousel x2). Total 64
cells: E34/P7/T15/Q8. Table A grows 20 to 21 (R-OUT-04
carry/follow); Table B grows 19 to 20 (R-OUT-03 cross); Table U
grows 11 to 12 (R-OUT-05 focused-output); Table C/W unchanged. The
matrix total is now 103 rows (99 + 4 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): three new strong
predicates stand matched or recorded, never silently differing.
R-OUT-03 cross-6/8 (B/I/X/S/A plus H partial) and target-X-5/8
(B/I/X/S/A), three families each: exhausted left focus crosses to L
and selects X; Ours crosses to X on
both platforms via the Engine proposal plus adapter actuation
(`S-ours-focus` + `S-ours-out`), matching the strong direction.
R-OUT-04 carry-8/8 plus follow-7/8 (all four families): explicit
output transfer carries, with visible mover focus in the declared
follow forms; xmonad retains the source view. Ours has no workspace-send
counterpart under the standing same-output-only Engine rule
(`S-ours-out` + `S-ours-ws`), while directional `CrossOutput` move
stays the separate R-OUT-01 verb. R-OUT-05 focused-output-7/8
(C/B/I/X/S/Q/A across all four families): ordinary admission lands on
the focused output with newcomer focus; Ours admission anchor is
sourced but output routing stays TBD. Recommend for batch user
review, no selection or code change: add an explicit output-send verb
through the shared Engine; establish the R-OUT-05/06 native journeys
before proposing changes there. The strong foreign directions are
recorded for any future output-transfer specification. Missing verbs
and TBD journeys never count as agreeing rejection. No product behavior
is changed by this assessment.

Multi-output live-test queue (22 applicable unresolved cells, grouped;
inventory/model checks precede fixtures; source-evidenced semantic legs
need no physical confirmation):

| IDs / profiles | Required discriminator / state | Why source evidence stops |
|---|---|---|
| R-OUT-03, COSMIC/Hyprland (2) | Focus left from A; shipped workspace layout (COSMIC), default fallback (Hyprland) | Fall-through/monitor-fallback sourced; exact branch/target TBD |
| R-OUT-03, paneru (1) | Focus West from A on D2 with D1/D2 strips | Cross-display `Focus` traversal untraced |
| R-OUT-04, xmonad (1) | Explicit send A to L; record source refocus | Carry plus target-stack focus sourced; `delete'` fallback untraced |
| R-OUT-05, Hyprland/niri/paneru/Ours KDE/Windows (5) | Pointer on R, focused L; open C; record landing output | Cursor-vs-active routing (Hyprland/niri), display routing (paneru), Engine routing plus native activation (Ours) untraced |
| R-OUT-06, all but karousel (13) | Occupied R focused; disconnect then reconnect; record evacuation and return | sway evacuation and bspwm default monitor retention/same-id reuse sourced; focus/visibility or affinity remainders and other migration journeys untraced |

## Mouse expansion: R-MOU-01..03 plus R-DRAG-01..08 scrolling backfill

Scope: this piece adds three GWT mouse scenarios (R-MOU-01 hover/click
focus, R-MOU-02 shared-edge resize, R-MOU-03 cross-output/workspace
drag), each with 14 Then profiles, and additive scrolling backfill
blocks for R-DRAG-01..08 (original wide tables preserved, with the
original A/B/C 640px widths, N-outside start, B focus, and A-edge
targets kept in the backfill Givens). Historical
tables and the 58-row audit above are preserved unchanged. Denominator,
families, and the strength rule are unchanged: consensus classification
below counts the original eight profiles only. The four scrolling
profiles form one correlated lineage reported as an explicit separate
non-voting comparison. A missing pointer verb never votes: absent edge
grabs, drag producers, and switcher targets are qualified or TBD, never
agreeing rejection. R-MOU-03 splits into a cross-output leg and a fresh
WS3-switcher leg: i3 and PaperWM carry E cross-output plus Q switcher,
and karousel carries Q
cross-output plus applicable-TBD switcher (all M). Keyboard carry/grid
verbs never count as pointer partial evidence, and verb inventories
(`S-ours-mou`, command enums) never count as behavior. Cell classes are
mutually exclusive per cell: E complete outcome evidenced with no TBD;
P one discriminator sub-leg evidenced with the remainder TBD and
queued; T TBD-only with a specific stop-tracing reason, never a bare
"untraced"; Q all legs qualified with pinned inventory evidence; M
mixed. E cells carry no TBD; every P/M cell names its explicit
remainder. Counts measure documentation coverage, not
votes. Semantic outcomes lead each Then in one to three lines.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-MOU-01 hover/click focus | plain click focuses B vs hover under the named ffm variant | click: C (press+deliver, `S-cos-dragpress`), H (CLICK refocus, `S-hyp-follow`), B (button1 ACTION_FOCUS, `S-bsp-ptrfocus`), I (`con_activate`, `S-i3-click`), X (`clickJustFocuses`, `S-xmo-ffm`), S (click focus, `S-sway-ffm`), Q (`focus_by_click` no-raise, `S-qti-click`), A (button1 activate, `S-awe-sloppy`) = 8/8, 4/4 fam; hover leg mode-dependent, no agreed predicate | U click-8/8 (Ours host journeys TBD both); hover C audit-only |
| R-MOU-02 shared-edge resize | dragged share change with clamp vs frame-only/no-resize | share change: B (fence split_ratio, `S-bsp-ptrresize`), I (pair percents, `S-i3-border`), S (pair fractions, `S-sway-rszedge`), A (mwfact, `S-awe-tresize`) = 4/8, 3 fam; no-share: X (bare nothing, mod floats, `S-xmo-mouse`); variant-gated: C (fork plus pixel path, `S-cos-resize`), H (default no-resize plus MBIND_RESIZE, `S-hyp-edgeresize`); no-counterpart: Q | C audit-only (no agreed predicate) |
| R-MOU-03 cross-output/workspace drag | cross-output insertion/follow; switcher drop | cross-output insert: I (same DT mechanics plus show+focus, `S-i3-tdrop`); partial insert/follow: C, H, B, S, A; TBD: X, Q; switcher Q: I, PaperWM | C audit-only (no agreed predicate) |

Scrolling comparison (non-voting): R-MOU-01 niri's shipped hover-off,
named enabled hover variant and plain-click activation/delivery are
evidenced; paneru FFM is partial (host click-focus TBD), while PaperWM
and karousel stay host-journey TBD. R-MOU-02
karousel dragged-width/stable-neighbor is complete at shipped
defaults; niri Mod+Right grab plus paneru modifier resize are partial
(shares TBD); PaperWM ResizeGrab is a native-Mutter journey marker
(TBD). R-MOU-03 niri output-tracking drop is partial (index TBD);
PaperWM zone insert is evidenced with a qualified switcher leg;
karousel is mixed; paneru is host-journey TBD. Backfill: niri carries
remove/reinsert/commit/activate legs across D01/02/04/05/06/07/08
with D03 partial (client titlebar and Mod+Left grabs differ in allowed
viewport scrolling); PaperWM carries zone/scratch/activate legs with
D05 complete and D06 final re-admission TBD; karousel carries
untile legs with D01 complete and D04/D05 host cancel/initiation TBD;
paneru is host-journey
TBD except D08 FFM-entry partial.

Counts (mutually exclusive E/P/T/Q/M). New rows (3x14=42): E 13, P 13,
T 12, Q 1, M 3. Original-eight new cells (3x8=24): E 11 (R-MOU-01: 6
H/I/X/S/Q/A; R-MOU-02: 5 B/I/X/S/A; R-MOU-03: 0), P 9 (R-MOU-01: 2 C/B;
R-MOU-02: 2 C/H; R-MOU-03: 5 C/H/B/S/A), T 2 (R-MOU-03: X/Q),
Q 1 (R-MOU-02: Q), M 1 (R-MOU-03: I).
Scrolling new cells (3x4=12): E 2 (R-MOU-01 niri, R-MOU-02 karousel), P 4
(R-MOU-01: paneru; R-MOU-02: niri/paneru; R-MOU-03:
niri), T 4 (R-MOU-01 PaperWM/karousel; R-MOU-02 PaperWM; R-MOU-03 paneru),
Q 0, M 2 (R-MOU-03 PaperWM/karousel). Ours new cells (3x2=6): T 6
(verb inventory only; every host producer journey TBD).
Backfill (8x4=32): E 2 (R-DRAG-01 karousel, R-DRAG-05 PaperWM),
P 20 (niri 8 plus PaperWM 7 plus karousel 4 plus paneru D08),
T 10, Q 0, M 0. Total 74 cells: E15/P33/T22/Q1/M3. Table U grows 12 to 13
(R-MOU-01 click-8/8); Tables A/B/C/W unchanged. The matrix total is
now 106 rows (103 + 3 new; backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands unresolved on both Ours platforms. R-MOU-01 click-8/8
(C/H/B/I/X/S/Q/A across all four original families): a plain click
focuses B. Ours adapter verbs (`S-ours-mou`, `S-ours-focus`) prove
requested-focus actions only, never the host click/hover producer
path, so both Ours cells stay applicable-TBD and the row sits in U,
never B. No other mouse predicate carries strong consensus (shares
split 4/8 with an xmonad no-share counter-vote; cross-output has no
agreed predicate). Recommend for batch user review, no selection or
code change: trace the host click-focus delivery plus hover policy,
the shared-edge share allocation, and the cross-output/switcher drop
journeys on both Ours platforms before proposing any behavior change;
establish the same three journeys for any future pointer-behavior
specification. Missing verbs and TBD journeys never count as agreeing
rejection. No product behavior is changed by this assessment.

## Special-windows expansion: R-SPC-01..05 (no backfill)

Scope: this piece adds five GWT special-windows scenarios (R-SPC-01
transient/modal, R-SPC-02 splash/utility, R-SPC-03 PiP, R-SPC-04
fixed-size, R-SPC-05 app-owned resize/hint change), each with 14 Then
profiles. The special area had no existing rows, so there is no
scrolling backfill to assess (0 existing rows, 0 cells). Fixed-size
citations in R-MIN-01..03 rows are reused by reference only, never
duplicated. PiP is app/flags-specific with no universal native type;
no profile invents one. Historical tables and the 58-row audit above
are preserved unchanged. Denominator, families, and the strength rule
are unchanged: consensus classification below counts the original
eight profiles only. The four scrolling profiles form one correlated
lineage reported as an explicit separate non-voting comparison. Cell
classes are mutually exclusive per cell: E complete outcome evidenced
with no TBD; P one discriminator sub-leg evidenced with the remainder
TBD and queued; T TBD-only; Q all legs qualified with pinned
inventory evidence; M mixed backend-dependent legs. E cells carry no
TBD; every P/M cell names its explicit remainder. Counts measure
documentation coverage, not votes. Semantic outcomes lead each Then in
one to three lines.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-SPC-01 transient dialog | transient dialog floats vs ordinary tiles | float: C, H, B, I, X, S, Q, A = 8/8, 4/4 fam | U float 8/8 (Ours KDE TBD, Windows excluded with host fence TBD) |
| R-SPC-02 splash/utility | excluded vs float vs tile; focus steal and switcher | float: H, I, Q, A plus C splash and S X11; tile: X, B (utility no-focus) plus C utility; backend-qualified: C, S | C audit-only (no agreed compound predicate) |
| R-SPC-03 PiP | native PiP type or rule | 0/8 (app/flags-specific everywhere) | C audit-only (no votes; niri Firefox rule non-voting) |
| R-SPC-04 fixed-size | float exception vs forced tile/clamp | float: C, H, B, I, X, S, Q, A = 8/8, 4/4 fam | A float 8/8 (Ours tiles both) |
| R-SPC-05 app resize/hint | ignored/authoritative vs hint-shaped vs clamp | authoritative-ignore: C, H, B, I, X, S, Q, A = 8/8, 4/4 fam; A later arrange hint-shaping qualifier | U ignore 8/8 (Ours authoritative both, remainders TBD) |

Scrolling comparison (non-voting): R-SPC-01 niri floats alongside
the parent, PaperWM floats above, karousel floats as transient-excluded,
paneru stays role-gated TBD. R-SPC-02 niri tiles both types, PaperWM
floats both, karousel/paneru TBD. R-SPC-03 niri floats only the
shipped Firefox PiP rule, others TBD. R-SPC-04 niri floats
fixed-height while PaperWM tiles ordinary, karousel floats as
untileable, paneru TBD.
R-SPC-05 niri/karousel clamp, PaperWM/paneru TBD.

Counts (mutually exclusive E/P/T/Q/M). New rows (5x14=70): E 24,
P 23, T 21, Q 0, M 2. Original-eight new cells (5x8=40): E 20
(R-SPC-01: B/X/Q/A; R-SPC-04: all eight; R-SPC-05: all eight),
P 10 (R-SPC-01: C/H/I/S; R-SPC-02: H/B/I/X/Q/A), T 8
(R-SPC-03: all eight), Q 0, M 2 (R-SPC-02: C/S Wayland/xdg typed
legs fixture-inapplicable with applicable xwayland legs). Scrolling
new cells (5x4=20): E 3 (R-SPC-01 niri; R-SPC-04 niri/PaperWM),
P 8, T 9, Q 0, M 0. Ours new cells (5x2=10): E 1 (R-SPC-04
Windows), P 5 (R-SPC-01 Windows; R-SPC-02 Windows
utility-excluded plus splash-TBD; R-SPC-04 KDE; R-SPC-05 both),
T 4 (R-SPC-01/02/03 KDE plus R-SPC-03 Windows), Q 0, M 0.
Table A grows 21
to 22 (R-SPC-04 fixed-float); Table B stays 20; Table U grows 13
to 15 full rows (R-SPC-01 transient-float, R-SPC-05 ignore) plus
the two KDE legs; Table C audit-only gains R-SPC-02/03; Table W
unchanged. The matrix total is now 111 rows (106 + 5 new; no
backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands against Ours, two stand unresolved. R-SPC-04
fixed-float 8/8 (all four families): reference WMs float min==max
windows while Ours KDE admits the Normal kind as an ordinary tile
and Ours Windows tiles with a declared-hint clamp. R-SPC-01
transient-float 8/8: Ours KDE eligibility is TBD (`normalWindow`
dialog mapping untraced) and Ours Windows excludes owned dialogs,
so no difference is established either way. R-SPC-05 ignore-8/8:
both Ours platforms keep allocation authoritative with TBD
remainders, matching the direction. Recommend for batch user
review, no selection or code change: decide whether Ours should
float fixed-size windows like the unanimous foreign direction;
trace KDE `normalWindow` dialog/splash/utility eligibility and the
Windows standalone-splash case before any transient/type decision;
no PiP or splash/utility choice is supported by consensus. Missing
type branches and TBD journeys never count as agreeing rejection.
No product behavior is changed by this assessment.

Special-windows live-test queue (46 applicable unresolved cells in
eight entries; source-evidenced semantic
legs need no physical confirmation):

| IDs / profiles | Required discriminator / state | Why source evidence stops |
|---|---|---|
| R-SPC-01, COSMIC/Hyprland/i3/sway/PaperWM/karousel (6) | Typed-dialog transient D for A; request focus A; fresh modal-D leg | Float admission sourced; parent-relative placement, newcomer focus (PaperWM/karousel) and modal fence untraced |
| R-SPC-01, paneru/Ours KDE/Windows (3) | Same fixture under role-gated management / `normalWindow` gate / Windows owned-dialog exclusion | Role and KDE dialog eligibility untraced; Windows host placement/modal fence untraced |
| R-SPC-02, Hyprland/bspwm/i3/xmonad/qtile/awesome/niri/PaperWM (8) | Typed splash U; fresh utility leg; both backends where split | Float/tile admission plus focus sourced; task-switcher presence untraced everywhere |
| R-SPC-02, COSMIC/sway (2 M-legs) | Same fixture per backend | xwayland float sourced; xdg typed legs fixture-inapplicable; focus/switcher untraced |
| R-SPC-02, karousel/paneru/Ours KDE/Ours Windows-splash (4) | Same fixture | KWin kind-flag mapping, role outcome, `normalWindow` eligibility and standalone-splash case untraced |
| R-SPC-03, all 14 (13 T + 1 P-remainder) | Playing media app; enter PiP; record toolkit/app/version, flags, rules | No native PiP type in any profile source; niri Firefox rule only |
| R-SPC-04, karousel/paneru/Ours KDE (3) | E with min=max 640x480 | Float/tile admission sourced; KWin resizeable mapping, paneru path and KDE native clamp untraced |
| R-SPC-05, niri/karousel/PaperWM/paneru/Ours KDE/Windows (6) | B requests 900x700; fresh min-hint leg | Authoritative allocation or clamp sourced; reactive reflow and host hint/resize paths untraced |

Queue cell math: 6 + 3 + 8 + 2 + 4 + 14 + 3 + 6 = 46, matching
P23 + T21 + M2 applicable unresolved cells (E24 need no tests).

## Activation expansion: R-ACT-01..02 (no backfill)

Scope: this piece adds two GWT activation scenarios (R-ACT-01
unsolicited activation request for a hidden window, R-ACT-02 urgency
marker set plus focus-time clear), each with 14 Then profiles. The
activation area had no existing rows, so there is no scrolling
backfill to assess (0 existing rows, 0 cells). R-WS-07 shell-selected
activation is reused by citation only, never duplicated: only the
native unsolicited-request/urgency handler paths count here, never
generic focus actuators or shell selection. Historical tables and the
58-row audit above are preserved unchanged. Denominator, families, and
the strength rule are unchanged: consensus classification below counts
the original eight profiles only. The four scrolling profiles form one
correlated lineage reported as an explicit separate non-voting
comparison. Cell classes are mutually exclusive per cell: E complete
outcome evidenced with no TBD; P one discriminator sub-leg evidenced
with the remainder TBD and queued; T TBD-only with a specific
stop-tracing reason; Q all legs qualified
(fixture-inapplicable / no-counterpart / owner-specific with pinned
inventory evidence). E cells carry no TBD; every P cell names its
explicit remainder. Counts measure documentation coverage, not votes.
Semantic outcomes lead each Then in one to three lines.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-ACT-01 hidden request | urgency-only marker vs switch-and-focus | switch-and-focus on the primary route: C (privileged-branch Workspace token plus shipped `Focus`, `S-cos-act`), B (`_NET_ACTIVE_WINDOW` honored at shipped default, `S-bsp-act`), X (default `doFocus` hook switches if necessary, `S-xmo-act`) = 3/8; urgency-only: H (both routes funnel into default-off `activate`, `S-hyp-act`), I (default `smart` on a hidden workspace, `S-i3-act`), S (focus-less-client token plus default `FOWA_URGENT`, `S-sway-act`) = 3/8, 3 fam; non-primary sandboxed COSMIC variant (urgency-only plus stale denial); TBD: Q/A (request routing untraced) | C audit-only (switch 3 vs urgency 3; neither leg strong) |
| R-ACT-02 urgency set/clear | marker without steal plus clear on focus | mark with evidence: H, B, I, S, Q (5/8, 3 fam) plus partial C (sandboxed workspace-level route) and A (marker only) = 7/8, 4 fam; clear: H (focus strips the bit), B (focus path clears), I (focus resets leaf/parents/workspace), S (focus clears or arms the clear timer), Q (focus resets plus strips the state atom) = 5/8, 3 fam; TBD: X (opt-in alternative only, no clear traced) | U mark-7/8 plus clear-5/8 (Ours native mark/clear TBD on both platforms) |

Scrolling comparison (non-voting): R-ACT-01 niri marks urgent-only
on the serial-less route at shipped default (no `on-xdg-activate`
rule; invalid-serial denial separate); PaperWM/karousel/paneru stay
TBD on the host activation journey. R-ACT-02 niri marks plus clears
on focus; PaperWM/karousel/paneru stay TBD (no mark/clear path
traced). No scrolling cell votes.

Counts (mutually exclusive E/P/T/Q/M). New rows (2x14=28): E 13,
P 2, T 13, Q 0, M 0. Original-eight new cells (2x8=16): E 11
(R-ACT-01: C/H/B/I/S/X; R-ACT-02: H/B/I/S/Q), P 2 (R-ACT-02: C
mark-only plus A marker-only), T 3 (R-ACT-01: Q/A; R-ACT-02: X),
Q 0. Scrolling new cells (2x4=8): E 2 (both niri), P 0, T 6, Q 0.
Ours new cells (2x2=4): T 4 (request routing plus native mark/clear
TBD on both rows and platforms). Table U grows 15 to 16 full rows
(R-ACT-02 mark/clear); Tables A/B/C/W unchanged. The matrix total
is now 113 rows (111 + 2 new; no backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands unresolved on both Ours platforms. R-ACT-02
mark-7/8 with evidence (5E plus C/A partial, four families) plus
clear-5/8 (H/B/I/S/Q across binary, tree, and
layout-driven families): urgency marks without stealing focus and
clears when the window is focused. Both Ours platforms leave the
native mark and the focus-time clear TBD: the KDE observer exposes
no attention signal with `setActive`-only actuation, and the
Windows adapter records foreground observation with no flash or
marker path. R-ACT-01 carries no strong consensus (primary switch 3
C/B/X vs urgency-only 3 H/I/S; sandboxed COSMIC variant
non-primary): Ours
request routing stays applicable-TBD on both platforms. Recommend
for batch user review, no selection or code change: establish the
host attention mark/clear journey on both Ours platforms before
proposing any urgency behavior; establish the unsolicited-request
switch-vs-marker outcome before any activation-routing decision.
Missing journeys never count as agreeing rejection. No product
behavior is changed by this assessment.

Activation live-test queue (15 applicable unresolved cells in four
entries; source-evidenced semantic legs need no physical
confirmation):

| IDs / profiles | Required discriminator / state | Why source evidence stops |
|---|---|---|
| R-ACT-01, qtile/awesome (2) | Hidden B, serial-less/X11 request; record switch vs marker and focus | Request routing untraced at these pins |
| R-ACT-01, PaperWM/karousel/paneru/Ours KDE/Windows (5) | Same fixture under host/Shell journeys and Engine observation | Shell/KWin/native activation journeys and Engine request routing untraced |
| R-ACT-02, COSMIC/awesome (2) | `H[A*,B]` with sandboxed B (COSMIC security context required); native mark path; focus B; record clear | Mark sourced; same-workspace (COSMIC) and any (awesome) focus-time clear untraced |
| R-ACT-02, xmonad/PaperWM/karousel/paneru/Ours KDE/Windows (6) | Same fixture; record marker and clear | Native hint/clear (xmonad) and host mark/clear paths untraced |

Queue cell math: 2 + 5 + 2 + 6 = 15, matching P2 + T13 applicable
unresolved cells (E13 need no tests; Q0).

## Restart expansion: R-RST-01..02 plus R-START-01..03/R-CTL-01..07 scrolling backfill

Scope: this piece adds two GWT restart scenarios (R-RST-01 orderly
owner restart with apps kept alive, R-RST-02 end session plus restore),
each with 14 Then profiles, and additive scrolling backfill blocks for
R-START-01..03 and R-CTL-01..07 (original wide tables preserved).
Historical tables and the 58-row audit above are preserved unchanged.
Denominator, families, and the strength rule are unchanged: consensus
classification below counts the original eight profiles only. The four
scrolling profiles form one correlated lineage reported as an explicit
separate non-voting comparison. Reload is never the restart journey:
in-place config reload legs (Hyprland, sway, qtile, niri) are cited as
the distinguished non-journey, never as recovery evidence. R-START
backfill assesses the original enable action only with no substituted
restart leg. R-RST-02 carries no focus predicate, and maximize-stateless
profiles run a fresh full leg there. Votes below count semantic
recovery legs (float, workspace, focus, ratio recovery) independently
of mechanism; partial legs vote only their established sub-leg.
Cell classes are mutually exclusive per cell: E complete outcome
evidenced with no TBD; P one discriminator sub-leg evidenced with the
remainder TBD and queued; T TBD-only with a specific stop-tracing
reason; Q all legs qualified (no-counterpart / fixture-inapplicable /
owner-specific with pinned inventory evidence); M mixed qualified plus
applicable-TBD legs. E cells carry no TBD; every P/M cell names its
explicit remainder. Counts measure documentation coverage, not votes.
Semantic outcomes lead each Then in one to three lines.

| Row | Predicate sub-legs (original eight) | Voters per sub-leg | Result |
|---|---|---|---|
| R-RST-01 float recovery | ordinary float status survives the restart | recover: B (float rectangles round-trip, `S-bsp-restore`), I (floating geometry round-trips, `S-i3-restart`), X (floating map resumes, `S-xmo-restart`), A (floating persists, `S-awe-ctl`) = 4/4ev, 3/4 fam; Q float outcome TBD, not a dissent vote | A recover-4/4ev (Ours resets intentional floats on both: session-local stores) |
| R-RST-01 workspace-set recovery | workspace set recovered | recover: B/I/X (desktops/tree/zipper restore), Q (group/screen names restore), A (shipped tags recreated from rc) = 5/5ev, 3/4 fam; membership is separate | U set-5/8 (Ours host-owned set handling TBD) |
| R-RST-01 membership recovery | tiled windows land back in their workspaces | recover: B/I/X (same-session ids re-match the restored tree) = 3/3ev, 3/4 fam; Q/A placement TBD, never a differ vote | U membership-3/3ev (Ours membership unknown) |
| R-RST-01 focus recovery | focused window recovered | recover: B (focused node plus history), I (focused flag with activation), X (StackSet focus resumes) = 3/3ev, 3/4 fam; Q TBD (nothing serialized, no evidence focus changes either) | U recover-3/3ev (Ours native focus TBD) |
| R-RST-01 ratio recovery | 70/30 shares recovered | recover: B/I/X vs reset-to-config: Q/A = 3/5, no majority either way | C audit-only |
| R-RST-02 session recovery | layout/workspace/native state recovered after apps acquire new ids | no established end-to-end original-eight outcome; same-session dumps and metadata do not establish replacement-id matching; max legs profile-specific with four qualified absent-max legs | C audit-only |

Scrolling comparison (non-voting): R-RST-01 karousel re-admits
fresh columns with no restore, PaperWM stages SaveState with existing
adoption, and paneru matches SessionRestore from the durable file
(`S-kar-rst`, `S-pap-rst`, `S-pan-rst`); niri has no layout-recovery
counterpart. R-RST-02 paneru has a durable metadata file but cross-session
matching is unresolved; the other scrolling session outcomes remain TBD.
Backfill: R-START-01..03
PaperWM/karousel assess the native enable journey with order and
second-leg remainders; niri/paneru have no enable counterpart.
R-CTL-01/02/05/06/07 are owner-specific qualified; R-CTL-03 is
owner-specific by fixture; R-CTL-04 reuses the established
no-workspace-float citations. Ours KDE freshly admits with classified
exceptions and Ours Windows freshly observes with settings-only
durability (`S-ours-kde-rst`, `S-ours-win-rst`).

Counts (mutually exclusive E/P/T/Q/M). New rows (2x14=28): E 3
(R-RST-01: B/I/X), P 11, T 7, Q 3, M 4. Original-eight new cells
(2x8=16): E 3, P 3 (R-RST-01: Q/A; R-RST-02: C), T 4 (R-RST-01: C;
R-RST-02: H/Q/A), Q 2 (R-RST-01: H/S), M 4 (R-RST-02: B/I/X/S
absent-max plus applicable full). Scrolling new cells (2x4=8): P 4
(R-RST-01: PaperWM/karousel/paneru; R-RST-02: paneru), T 3 (R-RST-02:
niri/PaperWM/karousel), Q 1 (R-RST-01: niri). Ours new cells (2x2=4):
P 4 (fresh admission with classified exceptions or settings-only
durability; native focus and host-max restore TBD). Backfill (10x4=40):
P 6 (R-START-01..03 PaperWM/karousel), Q 34 (R-START niri/paneru plus
R-CTL-01..07 all four each). Total 68 cells: E3/P17/T7/Q37/M4. Table A
grows 22 to 23 (R-RST-01 float recovery 4/4ev); Table U grows 16 to 19
full rows (workspace-set 5/8, membership 3/3ev, focus 3/3ev); Tables
B/C/W unchanged. The matrix total is now 115 rows (113 + 2 new;
backfill reuses IDs).

Ours-vs-consensus position (no behavior selected): one new strong
predicate stands differed on both Ours platforms. R-RST-01
float-4/4ev (B/I/X/A across binary, tree, and
layout-driven families): float state survives the owner restart.
Both Ours platforms reset intentional floats on restart (session-local
id set on KDE, session-local float store cleared on stop on Windows),
against the strong direction. R-RST-01 workspace-set-5/8, membership-3/3ev
and focus-3/3ev stand unresolved on Ours (set host-owned, membership and
native focus TBD). Recommend for batch user review, no selection or code change:
preserve intentional-float identity across owner restart; establish Ours membership,
native focus and host-max restore on both platforms first. Missing
verbs and TBD journeys never count as agreeing rejection. No product
behavior is changed by this assessment.

Restart live-test queue (28 applicable unresolved cells in four
entries; source-evidenced mechanism legs need no physical
confirmation):

| IDs / profiles | Required discriminator / state | Why source evidence stops |
|---|---|---|
| R-RST-01, qtile/awesome/karousel/PaperWM/paneru/Ours KDE/Windows (7) | WS1 70/30 with B focused, WS2, ordinary F; orderly restart with apps kept alive | Mechanisms sourced; exact ratios, frames, order and B focus TBD |
| R-RST-01, COSMIC (1) | Same fixture through an orderly compositor restart | No re-exec contract found at pin; journey applicability first |
| R-RST-02, all 14 (14) | Saved A/B plus F and native-max/full member across WS1/WS2; end session and restore | File/state mechanisms sourced where present; session-manager wiring plus exact layout and host-max restore TBD |
| R-START-01..03, PaperWM/karousel (6) | Four/five unmanaged floats; native enable; disable/re-enable | Enable adoption sourced; exact 2x2/cascade/minimum order and second-enable stability TBD |

Queue cell math: 7 + 1 + 14 + 6 = 28, matching P17 + T7 applicable-TBD
legs plus M4 applicable full legs (E3 need no tests; Q37 need no
live fixture: qualified legs never become live no-op tests).

## Column-mechanics closeout: R-COL-01..10 (compact)

Scope: ten GWT scrolling-column scenarios, each with 14 Then profiles
(140 cells). Tree-only fixtures have no strip counterpart, so most
original-eight cells are qualified (fixture-inapplicable or
no-counterpart with pinned inventory evidence); qtile's shipped
two-column limit qualifies the three-column R-COL-01 fixture. Two
original-eight partials: qtile R-COL-03 (directional shuffle carries
across columns, allocation and width recovery TBD) and R-COL-06
(`toggle_split` flips split/stacked, stacked selection TBD). No new
original-eight strong votes: denominator, families, and the strength
rule are unchanged, and scrolling profiles stay a supplemental
non-voting comparison. Citation keys resolve in the matrix index
(`S-nir-*`, `S-pap-*`, `S-kar-*`, `S-pan-*`, `S-ours-*`).

Counts (mutually exclusive E/P/T/Q/M): E 18, P 16, T 1, Q 105, M 0
(140 cells). Tables A/B/C/W/U unchanged. The matrix total is now 125
rows (115 + 10 new; no backfill reuses IDs).

Supplemental divergences (scrolling-lineage only, never consensus
votes): preset width cycles with wrap (niri, PaperWM, karousel,
paneru) vs no preset verb (Ours); consume/expel or slurp/barf column
join/leave (niri, PaperWM, karousel, paneru) with qtile's directional
shuffle as the only original-eight partial; minimal-fit vs named
centering viewport policies; whole-column workspace carry (niri,
karousel) vs single-window sends; paneru virtual-row moves and
app-native tab nesting (paneru-only predicates). Ours has no
column/strip admission, preset, consume/expel, viewport, center,
display-toggle, column-send, manual-scroll, row, or native-tab
counterpart on either platform (`S-ours-planops`, `S-ours-grp`,
`S-ours-resize`, `S-ours-move`, `S-ours-ws`); no column product model
is selected.

Column live-test queue: 17 applicable unresolved cells (qtile
R-COL-03/06; niri R-COL-01/07/08; PaperWM
R-COL-01/02/03/07/08; karousel R-COL-01; paneru
R-COL-01/03/07/08/09/10). The [archived change record](../changes/archive/reference-matrix-expansion.md)
consolidates the environment queue and fixtures. No product behavior is changed by this assessment.

## Strong-consensus rows with Ours TBD: native-observation list

Establish the host/adapter journey before any change; none is an
established difference.

- Insertion/anchor: R-INS-01 position, R-WS-02 after order, R-WS-04
landing, R-WS-05 float retention, R-CLOSE-02 fresh admission.
- Focus/raise: R-DRAG-08 press focus, R-MOU-01 click focus, R-FLT-12
raise path, R-FLT-14 free frame.
- Close: R-CLOSE-03 sole retention, R-CLOSE-04 float untouched + focus,
R-CLOSE-05 fullscreen close.
- Routing/urgency/persistence: R-OUT-05 focused-output routing, R-SPC-01
transient float, R-SPC-05 hint authority, R-ACT-02 urgency mark/clear,
R-RST-01 workspace set, R-RST-01 membership, R-RST-01 focus.
- KDE legs (Windows matches): R-WS-09 remembered-focus return, R-FLT-13
ordinary-float hidden.
- Additional semantic legs: R-INS-03 native activation, R-MAX-02 focus
enter/leave/restore sequence; R-FLT-05 sticky/restart visibility and origin
is a Table A native prerequisite. R-FLT-11 Windows remains implementation-pending,
so observation follows implementation. R-FLT-08 live confirmation,
R-MAX-04 acknowledgements/visuals and R-DRAG-03 exact pixels do not add
standalone tests: their semantic predicates are already established.

## Pinned-source-fill supplement (2026-10-09)

- Base matrix `04b818b`; source pins and `S()` citations remain in the
  [matrix legend](../spec/reference-outcomes.md#evidence-tags). Each row below
  links the filled cells containing its evidence. Counts are per stated leg,
  not whole-row consensus or a new requirement. Native frames/focus, missing
  fixture geometry, and unsupported compound legs remain TBD.
- Original eight voters only: C/H/B/I/X/S/Q/A, with the same four families.
  niri/PaperWM/karousel/paneru remain supplemental non-voters. Feature absence
  never votes as a no-op. Unestablished source-workspace index, destination
  contents, or effective axis excludes only the affected leg.
- Strength uses the header rule, including all-but-one of evidenced voters.
  In particular, cross-4/stay-1 across three families is strong; it does not
  fail merely because there are fewer than five agreeing voters.
- FLAG: these new counts qualify already-selected move/layout/fixed-window
  and migration requirements. Decisions, Ours cells and NORMATIVE rows are
  unchanged; any reconsideration requires user selection. Historical tables
  and their totals remain the dated audit snapshot.

| Source-filled predicate | Applicable source votes | Non-voting / unsupported leg | Strength and selected-row relevance |
|---|---|---|---|
| [R-MOV-09](../spec/reference-outcomes/move.md#r-mov-09-flat-swap-right-with-unequal-sibling-shares) flat sibling result | swap I,S = 2; wrap C = 1 | B establishes geometric peer swap, but exact N-ary embedding is unrecorded; H/X/Q/A exact fixture TBD/inapplicable | 2/3, two families only; weak. FLAG REQ-MOV-03 / same-axis setting; no default change |
| [R-MOV-10](../spec/reference-outcomes/move.md#r-mov-10-flat-swap-right-beside-a-group-neighbor) exact group-neighbor topology | enter I,S = 2; wrap C = 1 | B geometric leaf swap does not establish the exact N-ary start; H/X/Q/A exact fixture TBD/inapplicable | 2/3, two families only; weak. FLAG REQ-MOV-03 leaf/group boundary |
| [R-MOV-11](../spec/reference-outcomes/move.md#r-mov-11-sole-root-leaf-moves-up-to-the-adjacent-output) sole-root Up crossing | cross H,I,S,A = 4; stay Q = 1 | C workspace-first: cross only from first source workspace, index unspecified; B/X destination contents TBD | 4/5, three families; strong. FLAG REQ-MOV-08 / item 5 resolved by user 2026-10-09: KEEP crossing after local exhaustion all four directions; COSMIC Up is conditional (vertical workspace layout goes previous workspace first, crosses only from the first workspace), excluded from the favour count |
| [R-MOV-12](../spec/reference-outcomes/move.md#r-mov-12-exhausted-up-move-with-two-candidate-outputs-above) two-output Up action | select H,I,S,A = 4; stay Q = 1; explicit ambiguity refusal = 0 among those five | C reaches output selection conditionally; B/X swap-vs-miss depends on destination contents; selected output identity TBD | 4/5 selecting, three families; strong. FLAG REQ-MOV-08 / item 5 resolved by user 2026-10-09: window-based selection (shared edge containing moving window centre projection, else larger window-span overlap, final left/top tie-break); deliberate COSMIC deviation, no reference refuses |
| [R-MOV-13](../spec/reference-outcomes/move.md#r-mov-13-panel-work-area-gap-with-touching-full-output-rectangles) gap, Up leg | cross H,I,S,A = 4; stay Q = 1 | C source index unspecified; B/X destination contents TBD | 4/5, three families; strong. FLAG REQ-MOV-08 full-output selection |
| R-MOV-13 gap, Left leg | cross C,H,I,S,A = 5; stay Q = 1 | B/X destination contents TBD | 5/6, four families; strong. FLAG REQ-MOV-08 full-output selection |
| [R-LAY-05](../spec/reference-outcomes/layout-commands.md#r-lay-05-immediate-parent-toggled-twice-in-a-nested-tree) effective immediate-parent axis toggle | toggle C,B,I,S = 4 | H flips the parent bit but effective axis TBD under geometry recalc; X/Q/A no counterpart | 4/4, three families; strong. FLAG REQ-LAY-01 / item 4 |
| [R-LAY-06](../spec/reference-outcomes/layout-commands.md#r-lay-06-sole-leaf-toggle-then-ordinary-admission) sole toggle future-admission hint | no saved hint C,H,B = 3; saved hint I,S = 2 | X/Q/A no orientation-toggle counterpart | 3/5, three families but not strong by the header threshold. FLAG REQ-LAY-01: selected no-hint behavior retained |
| [R-OUT-07](../spec/reference-outcomes/multi-output.md#r-out-07-explicit-send-before-local-exhaustion-with-remembered-y) explicit transfer despite local neighbor | transfer C,H,B,I,X,S,Q,A = 8 | Target anchor/order and follow/stay are separate, differing or partial legs | 8/8, four families; strong. FLAG explicit output-send selection / item 5.3-5.4 |
| [R-SPC-06](../spec/reference-outcomes/special-windows.md#r-spc-06-admit-a-client-fixed-on-only-one-axis) one-axis fixed admission | tile C,B,I,X,Q,A = 6; float S = 1 | H splits by protocol: Wayland floats, X11 tiles | 6/7 unqualified votes, four families; strong. FLAG REQ-SPC-04a / Q2 D1: selected both-axis default aligns; either-axis alternative remains selected |
| [R-SPC-08](../spec/reference-outcomes/special-windows.md#r-spc-08-gain-or-lose-fixed-hints-after-admission) hint-change classification | admission-only C,H,B,I,X,S,Q = 7; reactive implicit A = 1 | Frames/focus TBD; explicit overrides are a different leg | 7/8, four families; strong. FLAG REQ-SPC-04b / Q2 D2 |
| [R-SPC-09](../spec/reference-outcomes/special-windows.md#r-spc-09-explicitly-tile-an-automatically-admitted-fixed-float) ordinary explicit tile | survives H,B,I,X,S,Q,A = 7; C directly tiles = 1 partial later-observation leg | C later reclassification remains TBD in this row; sticky equivalent is separate; supplemental karousel refuses | 7/7 fully established override votes, three families; strong. FLAG REQ-SPC-04c / Q2 D3; no sticky vote inferred |
| [R-WS-24](../spec/reference-outcomes/workspaces.md#r-ws-24-float-carry-sticky-stay) workspace float carry | carry C,H,B,I,S,Q,A = 7 | X lacks ownership-move counterpart; exact native frame/focus legs separate | 7/7, four families; strong. FLAG REQ-WS-12g / Q4 D7 resolved by user 2026-10-09: floats carry |
| R-WS-24 sticky member | source stay C,H,B = 3; carry I,S,A = 3 | Q sticky leg TBD; X lacks ownership-move counterpart | 3/3 split, no consensus. FLAG REQ-WS-12g / Q4 D7 resolved by user 2026-10-09: sticky stays on the source output (sticky belongs to the output; COSMIC) |
| [R-WS-25](../spec/reference-outcomes/workspaces.md#r-ws-25-overlay-refusal) fullscreen workspace carry | carry C,H,B,I,S,Q,A = 7 | X lacks ownership-move counterpart; native re-fit/mid-flight change legs TBD | 7/7, four families; strong. FLAG REQ-WS-12h / Q4 D8 |
| R-WS-25 maximize workspace carry | carry C,H,Q,A = 4 | B/I/X/S no applicable maximize state in this profile; native re-fit/mid-flight legs TBD | 4/4, three families; strong. FLAG REQ-WS-12h / Q4 D8 |
| [R-WS-27](../spec/reference-outcomes/workspaces.md#r-ws-27-two-candidate-migration-selection) directional whole-workspace migration action | select C,H,B,S = 4 | I/X/Q/A have no directional whole-workspace verb (explicit names/tags/screens only, or no ownership move at all); views/focus remain partial with exact runtime legs TBD in every directional cell; supplemental non-voters: niri selects (identity TBD), PaperWM directional verb with GNOME selection TBD, karousel fixture-inapplicable, paneru no-counterpart | 4/4ev, three families (n-ary, binary, tree); strong. FLAG REQ-WS-12 / Q4: project U1 stands as the selected largest-shared-edge-then-left/top rule, not a majority outcome |
| R-WS-27 two-candidate target identity | U1 H = 1 (longest-intersection 1200 vs 720) | C/B distance ties depend on unrecorded enumeration; S wlroots adjacency selection untraced; I/X/Q/A no directional counterpart | 1 vote; no consensus. H agrees with the project's largest-edge leg here, but its equal-edge enumeration tie-break does not establish left/top. COSMIC nearest-origin is the disclosed deliberate deviation, with identity TBD. The available evidence neither establishes a majority for U1 nor a contrary majority; the "follow the majority" rationale is unproven for this target rule |

- Other filled workspace-history, born-fullscreen, restart, B9 and M13 legs
  remain profile-specific or owner-specific. Their source mechanisms and
  unresolved outcomes are recorded in the matrix; this supplement does not
  manufacture a single vote for a compound fixture. R-START-03/R-MAX-01 and
  the existing R-WS-02/04/05 historical-vote predicates are unchanged by the
  added partial evidence.

## Cursor-first output-targeting supplement (2026-10-20)

Scope: Tier 1 only (COSMIC, Hyprland, i3, sway per the 2026-10-12
reference tiers) at pins C `3d55cba06c9cf6f27609cdefb520f7857dba20af`, H
`19fb395d45314960e6f79f17994a84094f1cd4f6`, I
`903bcd518df32b0e055b17f5da3f988a0187fd3d`, S
`1652c54b73f67df17b7b4ab0b0f7048204aa8104`, read via local sibling
clones (`git show`/`git grep` at pin; no web fetch). Tier2/3 profiles
were not investigated: their new-row cells stay TBD, never silent
agreement. karousel stays fixture-inapplicable (`S-kar-single`). Ours
cells stay TBD (cursor-first variant not established). Matrix evidence lives
in [R-OUT-05a](../spec/reference-outcomes/multi-output.md#r-out-05a-motion-then-ordinary-open),
[R-OUT-08](../spec/reference-outcomes/multi-output.md#r-out-08-numbered-switch-versus-send-with-opposite-cursor),
[R-OUT-09](../spec/reference-outcomes/multi-output.md#r-out-09-click-on-empty-area-of-the-other-output) and
[R-OUT-10](../spec/reference-outcomes/multi-output.md#r-out-10-focuscursor-coupling-defaults);
source keys `S-cos-cursor`, `S-hyp-cursor`, `S-i3-ffmoff`, `S-sway-ffmoff`
in the [matrix legend](../spec/reference-outcomes.md#evidence-tags).
The historical focused-7/8 R-OUT-05 tallies above are labeled
historical/map-only and are explicitly superseded for motion-first
legs by this supplement; they are not silently retallied. Nothing here
selects a product requirement: REQ-OUT-05 stays NORMATIVE with no
override, and every recommendation below is Tentative (user review
pending).

| Clause | Tier 1 per-clause summary | Recommendation |
|---|---|---|
| Motion then ordinary map / shortcut launch | C lands R splitting MRU Y (`active_output` follows motion); H lands R splitting the cursor-closest node (focus monitor follows motion); I/S with FFM disabled stay on L. Built-in shortcut spawns capture that same R/R/L/L destination in startup context, distinct from no-token map-time fallback. 2-2 split, no Tier 1 consensus | Tentative (user review pending): favor cursor-first under the tier rule (COSMIC primary, H agrees). Override of standing R-OUT-05 requires explicit user selection |
| Numbered switch with opposite cursor | Fresh numbers land R in C (trailing-empty index) and H (created on the focus monitor), L in I/S. Per-output relative-next advances R in C/H vs L in I/S. Existing global numbers stay owned (row caveat). 2-2 split; exact settled focus TBD | Tentative (user review pending): favor cursor-first under the same tier rule; explicitly decide how platform workspace ordinals map to outputs rather than copying global-ID lookup |
| Workspace send with opposite cursor | Source consensus 4/4: all act on focused A on L. Target scope splits: H `+1` resolves focus-monitor-relative (5 + 1 = ws 6 on R); C/I/S resolve focused-output-relative (ws 2 on L) | Tentative (user review pending): keep both subject and relative workspace resolution on the focused window's output, matching C/I/S; disclose H's contrary relative-target behavior |
| Same-output ordinary anchor | All four use focused A as the admission anchor. Split geometry differs (C old-first/new-after groups; H pointer-half order; I/S flat siblings) | Tentative (user review pending): retain focused-A anchoring; evidence supports the target, not identical recursive split geometry |
| Other-output insertion | C splits R's MRU Y; H splits the cursor-closest node (X despite MRU Y); I/S have no pointer-forced FFM-off branch. No universal insertion consensus | Tentative (user review pending): define "standard location" as that output's last-focused tiled anchor, following C; disclose H's cursor-closest alternative rather than claiming consensus |
| Focus-follows-cursor option/default | Configurable in all four. Default C off, H/I/S on; all three other Tier 1 references oppose C's default | Tentative (user review pending): support the optional setting. The tier-rule exception favors default-on; requested default-off is a deliberate deviation needing user resolution, not a Tier 1 default consensus. Existing normative click-focus behavior is not changed |
| Cursor-follows-focus option/default | C general cursor-following default off; H allows verb-specific warps but workspace-warp setting is 0; I/S default output warps on. Each has configurable suppression; these are not identical trigger scopes | Tentative (user review pending): support optional warping, but describe requested default-off as a choice against common cross-output warps, not a unanimous reference default; settle trigger scope explicitly |
| Empty click on the other output | Routing converges on R in all four after the click. Keyboard-focus effect splits 2-2: C/H retain A; I/S focus R's workspace | Tentative (user review pending): preserve R targeting; favor C's no-focus-write-on-empty behavior if a cross-platform click-focus policy is selected, disclosing I/S disagreement |

Unresolved/TBD claims: COSMIC `workspace_set` activate settled focus
and direction/layout-gate legs; i3/sway other-output anchor under
FFM-off beyond the focused-L policy; all Tier2/3 outcomes; all Ours
and native/live outcomes; exact pixel frames and focus-settle timing
in every new cell. GUI-launcher paths, existing-instance handoffs, unpropagated
startup tokens and focus/cursor changes between launch and map are outside
the captured-context launch fixture and remain TBD.
