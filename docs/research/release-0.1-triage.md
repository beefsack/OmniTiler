# Release 0.1 triage - user decisions and remaining tentative units

Status: 24 units approved by User (15 on 2026-10-10, nine on 2026-10-12);
four units plus Proposal B / N1-N4 remain tentative, pending user review.
Original proposal base HEAD `4281e73`, report commit `27a97c4`.
No live tests run. Rules: 0.1-relevant = touches must-pass core KDE journey
([decisions](../decisions.md#scope-and-platform-goals)) AND current KDE
undefined/refusal/no-op/inconsistent/surprising; post-0.1 = niche,
scrolling-only/COL, Windows/macOS-only, already acceptable, parked (LAY-04).
Current defaults/settings follow the user's 2026-10-12
[reference tiers](../decisions.md#reference-tiers); historical tallies below
are evidence, not tier-weighted selections. Earlier decisions stand unless revisited.
Gaming safety wins; overlap last resort; simplicity. TBD never invented.

Original proposal counts: 60/60 then-OPEN rows covered (32 relevant / 28
post-0.1); 28 decision units (D01-D27 + separate D28 MAX-09); 53/53 then-pending
checks (proposed M39 / K12 / W2). Batch 1 promoted 16 rows, leaving 44 OPEN.
Tentative decisions moved 16 of those to PROVISIONAL and split out KDE D28 as
REQ-MAX-09b. User 2026-10-12 promotes eleven rows: current spec
132 NORMATIVE / 6 PROVISIONAL / 28 OPEN.

## User decisions 2026-10-10

- Approved exactly recommendation (a): D02 (SPC-01), D04 (INS-03), D05
  (INS-05), D06 (OUT-05), D07 (CLOSE-02), D08 (CLOSE-03), D11 (MOV-01),
  D14 (RSZ-02, RSZ-03), D15 (MOU-01), D17 (WS-09), D18 (WS-02b, no
  setting), D19 (FLT-13), D20 (FLT-12, no lower verb for now), D21 (FLT-14),
  D24 (CLOSE-04). These 15 units / 16 rows are now NORMATIVE in the
  [spec](../spec/functional-spec.md); implementation and native evidence remain
  distinct ([batch-1 work](../backlog.md)).
- At that review, remaining 13 units pending: D01, D03, D09, D10, D12, D13,
  D16, D22, D23, D25, D26, D27, D28. Later selections are recorded below.
- Proposal B live-check classification (39 must-pass / 12 known-issue-allowed /
  2 Windows-release) and proposed new checks N1-N4 remain pending approval.
  Batch-1 verification checks added to the backlog are outside that original
  53-check inventory and are not classified by this unapproved proposal.
- Historical meta-question to raise with D03: do new settings (D03, D09, D10, D12,
  D13, D27) ship in 0.1, or ship defaults-only with settings later?

## User decisions 2026-10-12

- D01, D03, D09, D10, D12 and D13 are user-decided (eight NORMATIVE rows).
  Current rules are recorded once in [admission decisions](../decisions.md#placement-minimums-and-startup-adoption)
  and [move decisions](../decisions.md#move-layout-and-output-commands):
  D01 (a) native activation ownership plus CLOSE-05 (a) removal/refill/focus;
  D03 After focused plus per-leg re-application, no setting; D09 Geometric
  middle and D10 Wrap, each with a two-option post-0.1 setting; D12 Extract
  to same axis and D13 Wrap locally, no settings.
- The [tier rule](../decisions.md#reference-tiers) amends the 2026-10-07 rule.
  P2 settings now contain approved D09, D10 and D27. D03/D12/D13 settings and
  D09/D10 swap-target-leaf options are dropped.
- D27 (a) / REQ-ACT-01 / R-ACT-01 is user-decided: default `Mark urgent only`
  for unsolicited activation from an existing hidden-workspace window, with
  no workspace switch or focus change. This reverses tentative `Switch and
  focus`: Hyprland, i3 and sway (all Tier 1) agree against COSMIC under the
  tier rule. Fullscreen/game-focused requests are urgency-only in both modes.
  Post-0.1 `Activation request` setting: `Mark urgent only` (default; tooltip
  Hyprland, i3, sway) / `Switch and focus` (tooltip COSMIC, xmonad); the
  alternative qualifies via COSMIC Tier 1 plus xmonad Tier 2. Functional
  names/options; WM names only in tooltips. INS-07 no-steal covers tiling
  admission into inactive workspaces only, not existing-window activation;
  explicit Alt+Tab/taskbar selection is separate. [Decision](../decisions.md#activation-requests).
- D27 KDE first step: trace KWin's native Wayland xdg-activation and Xwayland
  behavior under default focus-stealing prevention. Attention-only means
  verification-only; native workspace/focus switching would make urgency-only
  an override conflicting with D01 native activation ownership, so stop and
  return to the user before code. Otherwise request/urgency handling needs code
  per the gap row; selection supplies no native evidence.
- D16 (a) / REQ-WS-05 / R-WS-05 is user-decided: sending a floating window
  to another workspace retains float and size on arrival, leaving destination
  tiles untouched, for follow/stay and numbered/relative sends. No setting:
  Hyprland, i3 and sway (Tier 1) plus xmonad, qtile and awesome (Tier 2) retain
  float against COSMIC fresh re-tile; COSMIC's alternative has no second
  Tier 1/2 reference. [Decision](../decisions.md#workspaces).
- D16 needs code: shared core `NotTiled` refusal and KDE tiled-only send
  eligibility remain gaps. Verify the untested native WS1 -> WS2 -> WS1
  roundtrip first; implement shared core first (serves Windows), then KDE
  adapter; Windows adapter work goes to handoff item 22. No native outcome
  is inferred from approval.
- D22 (a) / REQ-FLT-03 / R-FLT-03 is user-decided: floating out a tile
  preserves the relative proportions of surviving siblings (e.g. 50/25/25,
  float the middle 25 -> approximately 67/33). No setting. COSMIC observed
  (not source-proven), i3 and sway (Tier 1) agree; no evidenced alternative;
  Hyprland and Tier 2 remain untraced. Thin basis disclosed: i3/sway one
  family plus separate COSMIC observation. Verification-only: removal already
  retains survivor share ratios per the revision-bound gap assessment;
  native reflow live check remains pending.
  [Decision](../decisions.md#window-state-float-sticky-maximize-fullscreen).
- D01 needs a KDE convergence fullscreen focus fence; close and the
  D03/D09/D10/D12/D13/D22 defaults are verification-only. [Gap assessment](../backlog.md).
  Native outcomes remain TBD; selection does not establish implementation.
- Remaining tentative units: D23, D25, D26, D28;
  Proposal B / N1-N4 remain pending user review.

## Tentative decisions 2026-10-11 (pending user review)

Remaining tentative units only, distinct from the user decisions above.
No selection is approved until user review. No implementation is authorized by this section.
Historical proposals A1/A2/A3/B and reference evidence below are preserved
unchanged; source observations are not marked as approved.

- D23 (a) minimize releases allocation and reflows with a stored restore slot;
  restore to the old slot with no focus steal; sole-workspace minimize retains
  the workspace with focus none. Windows retains allocation: observed
  divergence, tentative shared parity target, not user-approved.
- D25 (a) re-observe membership, keep the host-owned set, restore focus only
  where provable.
- D26 (a) restore owned gaps/settings/intent and re-observe host
  set/membership/max; fallback (b) is a listed known limitation if the full
  leg proves nontrivial. Cross-login disclosure: approved REQ-RST-01d
  session-scoped store prohibits new-login/KWin hydration, so cross-login
  intent cannot currently restore; no implicit namespace expansion. Tentative
  fallback (b) applies to that leg unless the user authorizes a later design.
  Settings/gaps and host re-observe legs verify.
- D28 (a) keep the KDE fullscreen-send refusal as a listed 0.1 known issue
  pending the G-D2 native observation; no carry selected.
- Proposal B tentatively adopted as the original 53-check classification
  M39/K12/W2 plus N1-N4 as M checks. With distinct N1-N4 additions the
  inventory is 57 checks => M43/K12/W2. Batch-1 verification extras remain
  outside that original inventory and are not classified here.

### Historical tentative selections superseded 2026-10-12

- On 2026-10-11 D01 proposed ordinary newcomer focus over maximize and B
  remaining shown/focused over fullscreen; CLOSE-05 proposed removal/refill/focus.
  D03 proposed After focused plus per-leg re-application; D09/D10/D12/D13
  proposed the now-approved defaults. Native activation ownership supersedes
  D01's forced-retention wording.
- D27 proposed default Switch and focus plus a fullscreen/game-focused
  urgency-only guard. User 2026-10-12 selected Mark urgent only after tier
  rescoring, retaining the guard in both modes and a two-option post-0.1 setting.
- D16 proposed retain float as a 7/8 cross-family majority exception. User
  2026-10-12 approved retention of float/size and untouched destination tiles
  under the tier rule for follow/stay and numbered/relative sends, no setting;
  native roundtrip verification comes first.
- D22's ratio-preserved float-out reflow is now user-approved, no setting;
  the thin i3/sway one-family basis plus COSMIC observation remains disclosed,
  and native reflow evidence is pending.
- The six-setting defaults-only proposal (D03, D09, D10, D12, D13, D27)
  is superseded by the current P2 list above. Original options/tallies in A2/A3
  below remain historical proposals, not current settings commitments.

Reference key: C = COSMIC, H = Hyprland, B = bspwm, I = i3, X = xmonad,
S = sway, Q = qtile, A = awesome; `ev` = evidenced profiles, `fam` = families.
Consensus labels: B = strong consensus ours matches; U = strong consensus,
ours unresolved; W = numerical but not strong cross-family; C = listed
COSMIC differences without strong consensus; A = strong cross-family,
ours differs.
Tallies use [consensus](reference-wm-consensus.md), checked against the linked
per-WM action fills; observed COSMIC outcomes remain distinct from source
votes. Scrolling references do not vote. Inventory IDs come from the
[OPEN index](../spec/functional-spec.md#21-open-decisions-index).

## Proposal A1 - original exhaustive inventory (every then-OPEN row once)

V: R = relevant (decision unit), P = post-0.1. Spec line locators below are
historical at `27a97c4`; use requirement IDs after edits. Approval status is above.

| # | Row | V | Unit / post reason |
|---|---|---|---|
| 1 | REQ-INS-01b newcomer position (L74) | R | D03 joint with INS-04 |
| 2 | REQ-INS-03 first admission (L76) | R | D04 |
| 3 | REQ-INS-04 chained admission (L77) | R | D03 joint with INS-01b |
| 4 | REQ-INS-05 float-focus admission (L78) | R | D05 |
| 5 | REQ-INS-06 over overlay (L79) | R | D01 joint with CLOSE-05 |
| 6 | REQ-INS-07b inactive routing (L81) | P | acceptable base (no-switch/no-steal NORMATIVE); routing 0/8 TBD, no evidence to decide |
| 7 | REQ-INS-08 preselection (L82) | P | niche new verb; no Ours concept |
| 8 | REQ-FOC-01 directional tie (L91) | P | acceptable: stable-A matches COSMIC; MRU only weak W 3/4ev 2fam |
| 9 | REQ-FOC-02 single-output edge (L92) | P | acceptable: Edge-retain matches COSMIC + B stay-5/8 |
| 10 | REQ-FOC-03 cycle order (L93) | P | niche: no cycle verb at all |
| 11 | REQ-FOC-04 parent/child scope (L94) | P | niche: leaf-only, no container verb |
| 12 | REQ-MOV-01 flat-triple perpendicular (L106) | R | D11 |
| 13 | REQ-MOV-02 in-group swap (L107) | P | acceptable: matches 7/7 sourced |
| 14 | REQ-MOV-04 ancestor escape (L109) | R | D12 |
| 15 | REQ-MOV-05 edge no-op (L110) | P | acceptable: matches 7/7 sourced |
| 16 | REQ-MOV-06 nested entry (L111) | R | D09 |
| 17 | REQ-MOV-07 orthogonal escape (L112) | R | D10 |
| 18 | REQ-RSZ-02 outward edge (L123) | R | D14 joint with RSZ-03 |
| 19 | REQ-RSZ-03 nested scope (L124) | R | D14 joint with RSZ-02 |
| 20 | REQ-RSZ-04 equalize (L125) | P | niche: no verb; mechanisms differ |
| 21 | REQ-LAY-02 rotate/mirror (L133) | P | niche: no verb |
| 22 | REQ-LAY-03 promote-to-master (L134) | P | niche: no master state |
| 23 | REQ-LAY-04 layout select (L135) | P | parked user decision 2026-10-09 |
| 24 | REQ-WS-02b after-order (L149) | R | D18 |
| 25 | REQ-WS-05 floating transfer (L152) | R | D16 |
| 26 | REQ-WS-07 switcher listing (L154) | P | niche/owner-specific singleton; no design selected |
| 27 | REQ-WS-09 select WS2/WS1 (L156) | R | D17 |
| 28 | REQ-WS-10 empty middle (L157) | P | owner-specific, remove-3 vs retain-4, no consensus |
| 29 | REQ-FLT-03 survivor widths (L185) | R | D22 |
| 30 | REQ-FLT-12 raise/lower (L196) | R | D20 |
| 31 | REQ-FLT-13 ordinary float switch (L197) | R | D19 |
| 32 | REQ-FLT-14 pointer drag/resize float (L198) | R | D21 |
| 33 | REQ-MNZ-01 minimize middle (L230) | R | D23 joint MNZ-01/02/03 |
| 34 | REQ-MNZ-02 restore slot (L231) | R | D23 joint |
| 35 | REQ-MNZ-03 sole minimize (L232) | R | D23 joint |
| 36 | REQ-CLOSE-02 fresh reopen (L255) | R | D07 |
| 37 | REQ-CLOSE-03 sole close (L256) | R | D08 |
| 38 | REQ-CLOSE-04 float close (L257) | R | D24 |
| 39 | REQ-CLOSE-05 maximized/fullscreen close (L258) | R | D01 joint with INS-06 |
| 40 | REQ-OUT-02 perpendicular at edge (L281) | R | D13 |
| 41 | REQ-OUT-05 open two outputs (L284) | R | D06 |
| 42 | REQ-MOU-01 hover vs click (L303) | R | D15 |
| 43 | REQ-ACT-01 activation request (L312) | R | D27 |
| 44 | REQ-ACT-02 urgency mark/clear (L313) | P | niche + no host signal in either adapter |
| 45 | REQ-RST-01c set/membership/focus (L336) | R | D25 |
| 46 | REQ-RST-02 session restore (L340) | R | D26 |
| 47 | REQ-SPC-01 transient dialog (L348) | R | D02 |
| 48 | REQ-SPC-02 splash/utility (L349) | P | niche app-specific, no strong predicate |
| 49 | REQ-SPC-03 PiP (L350) | P | niche, no universal type/votes |
| 50 | REQ-SPC-05b hint reflow (L361) | P | acceptable direction (authoritative 8/8); only reflow remainder TBD |
| 51 | REQ-COL-01 over columns (L370) | P | scrolling-only/COL, no counterpart |
| 52 | REQ-COL-02 width presets (L371) | P | scrolling-only/COL, no counterpart |
| 53 | REQ-COL-03 consume/expel (L372) | P | scrolling-only/COL, no counterpart |
| 54 | REQ-COL-04 off-screen focus (L373) | P | scrolling-only/COL, no counterpart |
| 55 | REQ-COL-05 center column (L374) | P | scrolling-only/COL, no counterpart |
| 56 | REQ-COL-06 tabbed display (L375) | P | scrolling-only/COL, no counterpart |
| 57 | REQ-COL-07 send column (L376) | P | scrolling-only/COL, no counterpart |
| 58 | REQ-COL-08 scroll viewport (L377) | P | scrolling-only/COL, no counterpart |
| 59 | REQ-COL-09 virtual rows (L378) | P | scrolling-only/COL, no counterpart |
| 60 | REQ-COL-10 app-native tabs (L379) | P | scrolling-only/COL, no counterpart |

## Proposal A2 - original decision units (one row per then-OPEN row)

Columns: KDE at proposal | COSMIC | tally + action source | options ->
recommendation. The User decisions section identifies approved units; others
await approval. Current implementation status is in the spec/backlog.

### D01 fullscreen admission + close cleanup (INS-06, CLOSE-05)

- Research update 2026-10-12: [fullscreen admission and explicit focus](d01-fullscreen-admission.md)
  records pinned defaults, the tally, native KWin/Windows baselines, current
  convergence behavior and four options. It recommends native activation
  ownership rather than forcing fullscreen B to remain shown/focused. This
  challenged the historical tentative (a). User 2026-10-12 selected native
  activation ownership and CLOSE-05 (a); see [current decisions](#user-decisions-2026-10-12).

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| INS-06 ([spec](../spec/functional-spec.md#L79)) | admission over maximized/fullscreen TBD | TBD (all `B:full` legs TBD) | C audit-only; `B:max` no-counterpart B/I/X/S; [R-INS-06](../spec/reference-outcomes/insertion.md#r-ins-06-open-over-an-overlay-maximized-plus-fresh-fullscreen-variant) | (a) tile behind overlay with explicit focus/visibility rules, no writes/steal while fullscreen; (b) fail-closed queue until clear (confusing everyday-maximize no-ops); (c) disturb overlay (rejected: gaming). -> (a) for both states; joint native-cleanup live leg with CLOSE-05 |
| CLOSE-05 ([spec](../spec/functional-spec.md#L258)) | removal + desired focus sourced, native cleanup TBD | TBD (removal sourced, overlay cleanup TBD) | U full-leg-5/8 (H/B/I/Q/A) 3fam; [R-CLOSE-05](../spec/reference-outcomes/close.md#r-close-05-close-a-maximized-or-fullscreen-window) | (a) removal + survivor refill with focus; (b) explicit overlay-clear-first. -> (a); native fullscreen cleanup is the joint live leg |

### D02 transient dialog admission (SPC-01)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| SPC-01 ([spec](../spec/functional-spec.md#L348)) | `normalWindow` mapping untraced | floats (`is_dialog`) | U float-8/8 4fam; [R-SPC-01](../spec/reference-outcomes/special-windows.md#r-spc-01-open-a-transient-dialog-then-request-parent-focus-fresh-modal-variant) | (a) float transients; (b) tile (no votes). -> (a); Windows owned-dialog exclusion stays documented divergence |

### D03 newcomer position + chained admission (INS-01b, INS-04)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| INS-01b ([spec](../spec/functional-spec.md#L74)) | order after/before focus TBD | after (source-proven `S-cos-add`) | U after-5/7ev (C,B,I,S,A vs X,Q; H EF), 4fam; [R-INS-01](../spec/reference-outcomes/insertion.md#r-ins-01-ordinary-third-window-admission) | Setting `New window position`: After focused (C,B,I,S,A) / Before focused (X,Q tooltips). Default after. -> approve setting + default (Before qualifies: >1 non-scrolling, no lineage exemption) |
| INS-04 ([spec](../spec/functional-spec.md#L77)) | legs 2-3 topology/focus/geometry TBD | leg-1 policy only; legs 2-3 0/8 | C audit-only; [R-INS-04](../spec/reference-outcomes/insertion.md#r-ins-04-chained-admission-with-a-fixed-pointer) | (a) re-apply D03 policy per leg with live pointer/focus updates; (b) freeze leg-1 topology. -> (a); legs 2-3 via live-test route, nothing invented |

### D04 first admission (INS-03)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| INS-03 ([spec](../spec/functional-spec.md#L76)) | single leaf + desired focus; native activation TBD | tiles full area + focuses (source-proven) | B unanimous 8/8 4fam; [R-INS-03](../spec/reference-outcomes/insertion.md#r-ins-03-first-admission-on-an-empty-workspace) | (a) native activate newcomer; (b) desired-focus only (risks dead keys). -> (a) |

### D05 float-focus anchor (INS-05)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| INS-05 ([spec](../spec/functional-spec.md#L78)) | float holds no leaf; root-wrap fallback (differs) | anchor to tiling neighbor B (source-proven `S-cos-last`) | C audit-only; anchor C/I/S + H partial; B/X/Q/A TBD; [R-INS-05](../spec/reference-outcomes/insertion.md#r-ins-05-admission-while-an-ordinary-float-has-focus) | (a) anchor to nearest tiling neighbor (C/I/S direction); (b) keep root-wrap (inconsistent with approved long-edge-at-focus). -> (a) |

### D06 two-output open routing (OUT-05)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| OUT-05 ([spec](../spec/functional-spec.md#L284)) | routing TBD | focused output + newcomer focus | U focused-7/8 4fam; [R-OUT-05](../spec/reference-outcomes/multi-output.md#r-out-05-open-a-window-with-two-occupied-outputs) | (a) focused output + newcomer focus; (b) pointer routing (needs journey). -> (a) |

### D07 fresh reopen (CLOSE-02)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| CLOSE-02 ([spec](../spec/functional-spec.md#L255)) | reopen admission/focus TBD | fresh, no old-slot | U F8/8; [R-CLOSE-02](../spec/reference-outcomes/close.md#r-close-02-close-refocus-fresh-reopen) | (a) fresh admission; (b) old-slot restore (no votes). -> (a) |

### D08 sole close (CLOSE-03)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| CLOSE-03 ([spec](../spec/functional-spec.md#L256)) | Engine collapse sourced, native TBD | retains shown workspace | U retained-7/8 4fam; [R-CLOSE-03](../spec/reference-outcomes/close.md#r-close-03-close-the-sole-window-on-the-shown-workspace) | (a) retain shown workspace; (b) follow focus away (no votes). -> (a) |

### D09 nested move entry (MOV-06)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MOV-06 ([spec](../spec/functional-spec.md#L111)) | midpoint insert, index TBD | geometric middle index (source-proven `S(S-cos-move)`: `V[B,A*,C]`) | C audit-only 3-way: enter I/S vs leaf-swap B/A vs Q carry; [R-MOV-06](../spec/reference-outcomes/move.md#r-mov-06-move-into-a-nested-perpendicular-neighbor-with-remembered-child) | Setting `Nested move entry`: Geometric middle (COSMIC) / Remembered child (i3, sway; insertion side differs) / Swap target leaf (bspwm, awesome; exact target differs, awesome tile-projected). Insert grows the neighbor group; swap preserves its size. Q carry single: skip. Default middle. -> approve |

### D10 orthogonal escape (MOV-07)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MOV-07 ([spec](../spec/functional-spec.md#L112)) | wrap outcome TBD | wraps via R1 mismatch fork (source-proven `S(S-cos-move)`: `V[H[A,B*],C]`) | C audit-only 2-2: escape I/S vs down-swap B/A; [R-MOV-07](../spec/reference-outcomes/move.md#r-mov-07-orthogonal-escape-across-a-perpendicular-parent) | Setting `Orthogonal escape`: Wrap (COSMIC) / Escape outward (i3, sway) / Swap target leaf (bspwm, awesome; awesome tile-projected). Wrap changes the ancestor layout; escape promotes the mover; swap exchanges leaves without promotion. Default wrap. -> approve |

### D11 flat-triple perpendicular (MOV-01)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MOV-01 ([spec](../spec/functional-spec.md#L106)) | behavior TBD | observed restructure `V[H[A,C],B]` via R1 (`UT(2026-08-20)` + conformance S1) | C audit-only historical tie R2/N2: restructure I/S vs model-policy Q/A; current fills also establish B no-target no-op, Q/A exact flat-triple outcome TBD; [R-MOV-01](../spec/reference-outcomes/move.md#r-mov-01-perpendicular-move-of-a-flat-triple) | (a) restructure (COSMIC-observed, i3, sway); (b) no-op at edge (bspwm exact fixture). Q/A in-column policies have no ordinary flat-triple fixture: model differences, no setting. -> (a); changes root orientation rather than refusing an ordinary move |

### D12 ancestor escape (MOV-04)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MOV-04 ([spec](../spec/functional-spec.md#L109)) | escape/retain TBD | observed `H[A,B,C]` via R3 ascend + same-axis flatten (`UT(2026-08-20)` + S1-03) | C audit-only: retain H/B vs partial-extract I/S; Q carry + A swap/miss singletons skipped; [R-MOV-04](../spec/reference-outcomes/move.md#r-mov-04-same-axis-ancestor-escape) | Setting `Same-axis escape`: Extract to same axis (COSMIC-observed, i3, sway) / Retain nesting (Hyprland, bspwm; H reinserts into binary nesting, B swaps with C). Default extract. Known-issue hold alternative only with approval. -> approve |

### D13 perpendicular move at output edge (OUT-02)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| OUT-02 ([spec](../spec/functional-spec.md#L281)) | local-vs-cross TBD | observed no-cross, local R1 (`S21-01` authored observation) | C audit-only: wrap I/S vs cross B/A; Q local-split; H/X TBD; [R-OUT-02](../spec/reference-outcomes/multi-output.md#r-out-02-perpendicular-move-at-an-output-edge) | Setting `Perpendicular edge move`: Wrap locally (COSMIC-observed, i3, sway) / Cross output (bspwm, awesome). Default wrap (product-preferred COSMIC-observed; cross is 2/8, not extremely strong). -> approve |

### D14 resize edges (RSZ-02, RSZ-03)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| RSZ-02 ([spec](../spec/functional-spec.md#L123)) | outward no-op TBD | no-op (source-proven) | W weak 3/4ev 2fam (C,I,S vs Q redistribution); [R-RSZ-02](../spec/reference-outcomes/resize.md#r-rsz-02-outward-resize-at-the-work-area-edge) | (a) outward no-op; (b) redistribution (Q single - skip). -> (a) |
| RSZ-03 ([spec](../spec/functional-spec.md#L124)) | nearest-split TBD | nearest inner split (source-proven) | W weak 3/3ev 2fam (C,I,S); [R-RSZ-03](../spec/reference-outcomes/resize.md#r-rsz-03-nested-resize-scope-nearest-split-vs-ancestor) | (a) nearest-split; (b) ancestor scope (no votes). -> (a) |

### D15 hover vs click focus (MOU-01)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MOU-01 ([spec](../spec/functional-spec.md#L303)) | host journeys TBD | plain click focuses (press-focus) | U click-8/8 4fam; [R-MOU-01](../spec/reference-outcomes/mouse.md#r-mou-01-pointer-hover-vs-click-focus) | (a) click focuses, hover off (matches COSMIC shipped `focus_follows_cursor=false`); (b) hover-focus (no votes). -> (a) |

### D16 floating transfer (WS-05)

User 2026-10-12 selected option (a), retaining float and size with destination
tiles untouched, no setting, for follow/stay and numbered/relative sends.
See [current approval, tier rationale and roundtrip-first condition](#user-decisions-2026-10-12).
The original proposal/tally below is historical, not the current selection rule.

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| WS-05 ([spec](../spec/functional-spec.md#L152)) | roundtrip untested | fresh-readmit (T) - the outlier | U R7/8 retain (H,B,I,X,S,Q,A) vs COSMIC T; [R-WS-05](../spec/reference-outcomes/workspaces.md#r-ws-05-floating-transfer) | (a) retain float (7/8, 3+ families: extremely strong, majority-over-COSMIC exception); (b) COSMIC fresh-readmit. -> (a); roundtrip live leg first |

### D17 workspace return focus (WS-09)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| WS-09 ([spec](../spec/functional-spec.md#L156)) | shell-driven, return-focus TBD | restores remembered focus | B/U remembered-7/8 (C,B,I,X,S,Q,A; H pointer-dependent); [R-WS-09](../spec/reference-outcomes/workspaces.md#r-ws-09-select-ws2-select-ws1-return-focus-and-viewport) | (a) restore remembered focus; (b) shell default (current TBD). -> (a); KDE native leg pending |

### D18 return after-order (WS-02b)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| WS-02b ([spec](../spec/functional-spec.md#L149)) | exact after-order TBD | B after A (target MRU) | U after-4/5ev (C,B,I,S vs X before; H,A excluded - no anchor; Q other leg, not a vote); [R-WS-02](../spec/reference-outcomes/workspaces.md#r-ws-02-send-back-and-return-anchor) | (a) after; (b) before (X single-evidence - skip). -> (a), no setting |

### D19 ordinary float across switch (FLT-13)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| FLT-13 ([spec](../spec/functional-spec.md#L197)) | KDE native TBD (Win hides) | per-workspace floats, hidden while away | B/U hidden-8/8 4fam; [R-FLT-13](../spec/reference-outcomes/floating.md#r-flt-13-ordinary-float-across-a-workspace-switch) | (a) hidden-while-away; (b) carry visible (no votes). -> (a); KDE native leg pending |

### D20 raise/lower floats (FLT-12)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| FLT-12 ([spec](../spec/functional-spec.md#L196)) | raise TBD, no lower path | focus raises | U raise-7/8 (C,H,B,I,S,Q,A; X layer-only); lower only W 2/2ev 1fam (Q/A); [R-FLT-12](../spec/reference-outcomes/floating.md#r-flt-12-raise-and-lower-overlapping-floats) | (a) focus raises; defer a project lower verb as a niche new action; (b) add lower verb (extra shortcut/action surface). Q/A establish verb availability, not competing policies for the same action; no settings exemption based on family count. -> (a) |

### D21 pointer float drag/resize (FLT-14)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| FLT-14 ([spec](../spec/functional-spec.md#L198)) | host TBD, project resize refuses | floating move/resize grabs keep free frame | U free-8/8 4fam; [R-FLT-14](../spec/reference-outcomes/floating.md#r-flt-14-drag-and-resize-a-float-by-pointer) | (a) host path keeps frame, project keeps refusing (matches all 8 + current); (b) project resize for floats (new surface, no votes). -> (a) |

### D22 float-out survivor widths (FLT-03)

User 2026-10-12 selected option (a), preserving survivor proportions, no setting.
See [current approval, tier rationale and verification-only status](#user-decisions-2026-10-12).
The original proposal/tally below is historical; fallback (b) is not selected.

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| FLT-03 ([spec](../spec/functional-spec.md#L185)) | removal reflow TBD | observed ratio-preserved (ER observed; thin 2/2ev 1fam I/S) | W thin; [R-FLT-03](../spec/reference-outcomes/floating.md#r-flt-03-float-out-survivor-widths) | (a) ratio-preserved reflow (follow COSMIC-observed, disclosed thin); (b) hold TBD-known past 0.1 (needs approval). -> (a) preferred, (b) explicit fallback |

### D23 minimize + restore (MNZ-01, MNZ-02, MNZ-03)

| Row | KDE current | COSMIC (source-proven) | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| MNZ-01 ([spec](../spec/functional-spec.md#L230)) | allocation TBD (Win retains) | B unmaps from tree, siblings reflow, into `MinimizedWindow::Tiling` restore state; focus to MRU C (`S(S-cos-minimize)` + focusfix) | scoped 3/3ev: C/Q/A vacate allocation; H/B/I/X/S no-counterpart (current fills supersede older TBDs; absence non-voting); [R-MNZ-01](../spec/reference-outcomes/minimize.md#r-mnz-01-native-minimize-of-a-middle-tile) | (a) release allocation + reflow with stored restore slot; (b) retain allocation (Windows-DIFFERENT, no reference support). -> (a); record Windows divergence |
| MNZ-02 ([spec](../spec/functional-spec.md#L231)) | slot TBD (Win old-slot) | old slot via remap; no focus write (C stays) | old-slot 2/2ev (C,A) 2fam, scoped; Q path-only; [R-MNZ-02](../spec/reference-outcomes/minimize.md#r-mnz-02-native-restore-of-the-minimized-tile) | (a) restore old slot, no focus steal; (b) fresh admission (no votes). -> (a) |
| MNZ-03 ([spec](../spec/functional-spec.md#L232)) | sole-minimize TBD | stored as tiling restore state, workspace retained, focus none (source-proven, no established multi-WM tally) | C audit-only (COSMIC legs source-proven; no multi-WM tally); [R-MNZ-03](../spec/reference-outcomes/minimize.md#r-mnz-03-native-minimize-of-a-sole-workspace-window) | joint with MNZ-01/02: retain workspace + stored restore + focus none. -> approve as joint consequence, no independent tally claimed |

### D24 float close (CLOSE-04)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| CLOSE-04 ([spec](../spec/functional-spec.md#L257)) | exception-drop sourced, adapter TBD | tiles untouched + MRU refocus | U untouched-6/8 + focus-6/8 4fam; [R-CLOSE-04](../spec/reference-outcomes/close.md#r-close-04-close-a-focused-ordinary-float-over-tiles) | (a) untouched + MRU refocus; (b) reflow all (no votes). -> (a) |

### D25 restart set/membership/focus (RST-01c)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| RST-01c ([spec](../spec/functional-spec.md#L336)) | set/membership/focus TBD | n/a (host set persists independently) | U: set 5/8, membership 3/3ev (B/I/X), focus 3/3ev (B/I/X), scoped; [R-RST-01](../spec/reference-outcomes/restart-persistence.md#r-rst-01-orderly-owner-restart-with-apps-kept-alive) | (a) re-observe membership, keep host-owned set, restore focus where provable; (b) persist/restore layouts (rejected: never selected). -> (a) bounded 0.1 position; legs need Ours journey checks |

### D26 session restore (RST-02)

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| RST-02 ([spec](../spec/functional-spec.md#L340)) | gaps/settings restore; host-max TBD | TBD (no established end-to-end outcome; consensus C audit-only supersedes the stale spec Table-U label - spec not edited here) | C audit-only; [R-RST-02](../spec/reference-outcomes/restart-persistence.md#r-rst-02-end-session-restore-session-and-apps) | (a) restore owned state (gaps/settings/intent), re-observe host set/membership/max, no invented layout restore; (b) hold end-session restore past 0.1 as listed known limitation (needs approval). -> (a) with (b) as explicit fallback; no reference support invented either way |

### D27 activation request (ACT-01)

User 2026-10-12 selected option (a) with the tier-rescored default Mark urgent
only, both-modes gaming guard and two-option post-0.1 setting; see
[current approval and KDE trace-first condition](#user-decisions-2026-10-12).
The original proposal/tally below is historical, not the current default.

| Row | KDE current | COSMIC | Tally / source | Options -> recommendation |
|---|---|---|---|---|
| ACT-01 ([spec](../spec/functional-spec.md#L312)) | journey TBD | ordinary: switches to WS1, focuses B (source-proven `S(S-cos-act)`); sandboxed variant: urgency-only, stale denied | C audit-only: switch-3 (C,B,X) vs urgency-3 (H,I,S); Q/A TBD; [R-ACT-01](../spec/reference-outcomes/activation.md#r-act-01-hidden-window-sends-an-unsolicited-activation-request) | Setting `Activation request`: Switch and focus (COSMIC, bspwm, xmonad) / Mark urgent only (Hyprland, i3, sway). Scopes: INS-07 no-steal covers tiling-admission into inactive workspaces only - not overapplied to existing-window activation. Gaming guard: urgency-only while a fullscreen/game window is focused (gaming safety justifies this guarded deviation). Default switch. -> approve |

### D28 fullscreen send observe-first (MAX-09, separate)

MAX-09 maximized leg NORMATIVE (flag-stable carry delivered offline;
[spec](../spec/functional-spec.md#L223)); fullscreen leg OPEN observe-first,
currently refuses fullscreen sends. Table A carry-5/8 (B,I,S,Q,A) 3fam
([R-MAX-09](../spec/reference-outcomes/maximize-fullscreen.md#r-max-09-send-a-maximizedfullscreen-window-to-another-workspace)); H/C
TBD; COSMIC carry untraced; KDE native-send untraced.
Explicit approval ask (no product change proposed): (a) hold the current
fullscreen refusal for 0.1 as a listed known issue pending the G-D2 native
observation (fail-closed, gaming-safe); (b) gated carry behind observation
risks fullscreen writes during games. -> (a); carry policy selected only
after observation.

## Proposal A3 - ordered decision summary (28 units, one row each)

| Unit | Rows | COSMIC | Recommendation |
|---|---|---|---|
| D01 | INS-06, CLOSE-05 | TBD both | tile-behind-overlay + removal/refill; joint native-cleanup live leg |
| D28 | MAX-09 (separate) | untraced | hold refusal as listed issue pending observation |
| D02 | SPC-01 | floats | float transients |
| D03 | INS-01b, INS-04 | after | setting After/Before (X,Q tooltips) + per-leg re-application |
| D04 | INS-03 | tiles+focuses | native activate newcomer |
| D05 | INS-05 | anchor-to-B | anchor to tiling neighbor |
| D06 | OUT-05 | focused output | focused output + newcomer focus |
| D07 | CLOSE-02 | fresh | fresh admission, no old-slot |
| D08 | CLOSE-03 | retained | retain shown workspace |
| D09 | MOV-06 | middle index | default Geometric middle; settings Remembered child / Swap target leaf |
| D10 | MOV-07 | wrap | default Wrap; settings Escape outward / Swap target leaf |
| D11 | MOV-01 | observed restructure | restructure; bspwm no-op alternative; Q/A exact-TBD model policies excluded |
| D12 | MOV-04 | observed extract | setting Extract / Retain nesting (H, bspwm) |
| D13 | OUT-02 | observed no-cross | setting Wrap locally / Cross output (bspwm, awesome) |
| D14 | RSZ-02, RSZ-03 | no-op + nearest | outward no-op + nearest-split |
| D15 | MOU-01 | click focuses | click focuses, hover off |
| D16 | WS-05 | outlier (fresh) | retain float (7/8 majority exception) |
| D17 | WS-09 | remembered focus | restore remembered focus |
| D18 | WS-02b | after | after, no setting (X single) |
| D19 | FLT-13 | hidden | hidden-while-away |
| D20 | FLT-12 | focus raises | focus raises; no lower verb |
| D21 | FLT-14 | free frame | host path keeps frame; project refuses |
| D22 | FLT-03 | observed ratio | ratio-preserved reflow (thin, disclosed) |
| D23 | MNZ-01, MNZ-02, MNZ-03 | release+reflow / old slot / stored+retained | release allocation + old-slot restore; record Win divergence |
| D24 | CLOSE-04 | untouched+MRU | tiles untouched, MRU refocus |
| D25 | RST-01c | n/a | re-observe membership, host set, provable focus |
| D26 | RST-02 | TBD | restore owned, re-observe rest (fallback: listed limitation) |
| D27 | ACT-01 | switch (ordinary) | setting Switch / Urgent-only + fullscreen-focused guard |

## Proposal B - pending live checks (53 top-level; M39 / K12 / W2)

Gate: VISION sleep/wake, plug/unplug, resolution/scaling, fullscreen apps,
underlying config ([VISION](../../VISION.md#reliability)); core tiling,
focus/move, workspaces, multi-output, border, shortcuts, settings, restart;
FX = missing/failed/removed effect keeps tiling working + KWin stable.

Verdicts: M = MUST-PASS; K = KNOWN-ISSUE-ALLOWED, only if listed and no
crash or silent tiling stop; W = Windows-release, outside KDE 0.1.

| # | Check ([backlog](../backlog.md#pending-live-checks)) | V | One-line reason |
|---|---|---|---|
| 1 | Reference comparison G-05/G-06/G-37/G-D2 | M | all four normative-selected core legs: G-06 focus/move mutex, G-D2 maximized carry, G-05 predicate, G-37 source refill |
| 2 | Two-candidate output move/send/migration | M | multi-output core (selection + arrival + fences) |
| 3 | Placement physical acceptance | W | Windows session dogfood (Windows gate) |
| 4 | Windows parity/settings/tray physical | W | Windows session incl Snap/Explorer/DPI (Windows gate) |
| 5 | B9/R-FLT-06 on COSMIC | K | explicit confirmation-only, not a gate |
| 6 | B1/B2 repeats + held-key autorepeat | M | core maximize/sticky toggles + shortcuts integrity |
| 7 | B9 unfloat-while-maximized KDE | M | selected B9 behavior needs native timing proof |
| 8 | D7 tile-override restart | M | restart core (intent persistence) |
| 9 | D5 born-fullscreen exit incl games | M | gaming non-interference + VISION fullscreen apps |
| 10 | D6 tiling enable with fixed windows | M | core tiling/workspace-enable under Q2 rules |
| 11 | Q3 R-RST-01/03/04 | M | restart core (intent/frame/store) |
| 12 | D1 settings UI | M | settings core (labels, live pickup, no-reclassify) |
| 13 | Q2 R-SPC-04 fixed admission | M | core admission incl borderless game, no-touch writes |
| 14 | R-WS-08/11 item 1 remainder | M | workspaces/multi-output core (scope, hotplug, presets) |
| 15 | R-WS-01/14 item 2 | M | workspaces send core (follow/stay, wrap, fill) |
| 16 | R-MOV-03 item 3 | M | move core + settings live-reread |
| 17 | R-DRAG-08 KDE press-focus | M | decided press-focus needs timing proof |
| 18 | Q3 born-maximized + B6 | M | core admission + minimums |
| 19 | R-MAX-03 Q3 scope | M | maximize/floating core (slot, settlement) |
| 20 | Tray workspace toggle remainder | M | workspaces core (toggle + default seeding) |
| 21 | Active border applet popups | M | border core (suppression + restore) |
| 22 | Host settings Revert + tray indicator | M | settings core + VISION config-change round-trip |
| 23 | Confirmed-2026-09-29 block remainder | M | settings/underlay/drag remainder; installed leg rides P1 packaging |
| 24 | Quiet refresh logs | K | observability cosmetics, no behavior impact |
| 25 | Same-output drag + preview remainder | M | decided Esc/cancel verdicts are core drag safety |
| 26 | Drop-intent edge leftovers | K | edge-case remainders (snap-back/Esc/size-increment/p13) absent crash/tiling stop |
| 27 | Convergence step-1 leftovers | M | core close/reopen focus + state restoration + rapid commands |
| 28 | Resilience batches | M | FX failed-endpoint/restart coverage (partial; see N1 gaps) |
| 29 | Tray Active status (`just dev`) | K | status display only; systemd legs ride external validation |
| 30 | Process-loss + sleep (single) | M | VISION sleep/wake directly |
| 31 | Native border delivery/suppression | M | border core + fullscreen suppression (gaming) |
| 32 | Sticky adoption restart | M | restart core (sticky identity) |
| 33 | Non-visible workspace tiling | M | approved background tiling (hidden admission, no steal) |
| 34 | Reliability gates (gaming + work-area) | M | VISION gaming/fullscreen + resolution/scaling (M16 user-owned) |
| 35 | Custom Tile manual runtime | K | separately authorized dev-component check |
| 36 | Nested placement visual smoke | K | visual affordance only |
| 37 | Tray live/release acceptance | K | tray robustness + packaging-dependent login/autostart; ships listed absent crash/tiling stop |
| 38 | Drag-oracle post-fix proof | K | diagnostic proof; behavior covered by 18/25 |
| 39 | Startup adoption | M | restart/startup core (fit vs seed after Planner loss) |
| 40 | KWin controller silent unload | K | diagnostic-only by definition |
| 41 | Correlated observability capture | K | diagnostics coverage |
| 42 | Q4 migration | M | multi-output/workspaces core |
| 43 | Item 5 R-MOV-08/R-OUT-04 | M | multi-output move/send core |
| 44 | K1 visual remap remainder | M | multi-output border/underlay/preview across scales |
| 45 | Born-fullscreen dogfood | M | gaming/fullscreen core |
| 46 | Directional + shortcut + convergence re-test | M | focus/move + Apply/Force/Revert + convergence core |
| 47 | Cross-output drops | M | multi-output drag core |
| 48 | KWin restart surviving Planner | M | restart core (fresh-adopt, tray recovery) |
| 49 | Reconciliation phase 1 | M | multi-output stability (no ping-pong/departure) |
| 50 | Process-loss + sleep (multi) | M | VISION sleep/wake on multi-output |
| 51 | Output hotplug displacement/return | M | VISION plug/unplug directly |
| 52 | Multi-output anti-oscillation | K | trailing-empty lifecycle edge; ships listed absent oscillation-caused stop |
| 53 | Native dev lifecycle removal/dogfood | K | dev-setup path; FX removal leg covered by 28/N1 |

Packaging (separate, not in Pending checks): external NixOS/HM validation
P1 in gate; mainstream packaging P1 per 2026-10-10 scope.

### Gate coverage vs exact gaps

| Gate category | Existing checks | Exact gap -> new check (M) |
|---|---|---|
| sleep/wake | 30, 50 | covered (add game-present combo only: N3) |
| plug/unplug | 51 (+14 hotplug legs) | covered |
| resolution/scaling | 34 (work-area gate), 44 | covered; overlay interplay untested: N4 |
| fullscreen apps | 9, 13, 45, 34 | covered |
| underlying config | 22 (Fix/Revert flow), 12 (Save path) | external/bypass edits untested: N2 |
| gaming non-interference | 9, 31, 34, 45 | covered (+N3 combo) |
| tiling/focus/move/ws/multi/border/shortcuts/settings/restart | 1, 2, 6-8, 10-23, 25-27, 31-33, 39, 42-49 | covered |
| FX missing/failed/removed effect | 28 (failed endpoints), 53 (partial) | missing-at-startup, explicit disable, package remove untested: N1 |

Proposed new checks (user-owned; additional legs, not a new release gate):

| Check | Verdict | Action | Expected result |
|---|---|---|---|
| N1 effect-absent | M | Start with effect missing/disabled; disable mid-session; remove optional effect package; exercise failed endpoint with check 28 | Tiling keeps working throughout; KWin stable |
| N2 external config | M | Change host configuration outside Settings, then reconfigure | Validated reconcile without unintended writes; Fix/Revert ownership respected; no crash or silent stop |
| N3 sleep + gaming | M | Sleep/wake with fullscreen game and tiled session | Game unimpeded; tiling recovers; no stuck holds |
| N4 scaling + overlays | M | Change scale with maximized/fullscreen/fixed-float windows | Overlays retained; no fullscreen geometry writes; quiet settlement |

## Limitations

No live tests run. Tallies reproduce via consensus voter lists + anchored
action rows. Thin/disclosed: FLT-03 (COSMIC-observed, 1-family source),
MNZ-02 (2/2ev scoped), MNZ-03 (COSMIC source-proven, no multi tally),
INS-07b (0/8, post). Surprises: WS-05 COSMIC-outlier (majority exception);
MNZ-01 direction corrected (release, not retain); OUT-02 COSMIC-observed
wrap (not cross); RST-02 consensus C audit-only, with the new tentative clause
recorded separately from reference evidence.
