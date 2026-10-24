# Decisions archive (historical, not current policy)

Consolidated 2026-10-24. Everything here is superseded or completed
history. Current user-approved product decisions live in
[decisions.md](decisions.md); where a current section exists it is linked
from each entry below. Nothing here authorizes implementation.

- Core + Windows world-model rearchitecture (v2) is COMPLETE, with final
  native missing proofs accepted:
  [closing outcome](changes/archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24).
- Durable contracts: [adapter guide](porting/world-adapter-guide.md),
  [KDE notes](porting/kde-port-notes.md),
  [macOS notes](porting/macos-port-notes.md). KDE/macOS remain unported.
- Full stage-by-stage evidence stays in
  [core-world-model-rearchitecture](changes/archive/core-world-model-rearchitecture.md);
  this file holds only the decision-text excerpts moved out of
  `decisions.md`, verbatim except for these headers.

## 1. Windows per-domain API / ManagedWorkspaces / v1 authority (historical)

Superseded 2026-10-24 by the delivered v2 boundary. Current:
[decisions.md](decisions.md#engine-architecture-and-convergence),
[decisions.md](decisions.md#core-platform-boundary),
[decisions.md](decisions.md#logical-workspace-authority).

### 1a. Transitional domain API (Workspaces, moved 2026-10-24)

> - Core plus KDE delivered offline 2026-10-17
>   ([contract, evidence and live checks](changes/archive/core-full-state-replies.md)).
>   Windows/macOS behavioral adoption remains separate; their transitional
>   domain API remains available.

Current: [decisions.md](decisions.md#workspaces).

### 1b. Retained-Engine multi-output wording (moved 2026-10-24)

> - User 2026-10-14 approved as delivered: cross-output keyboard moves and
>   native title-bar releases plan on a candidate Engine (Win+drag removed
>   2026-10-18). Retained Engine state
>   and workspace membership publish only after fresh topology, geometry
>   readback and arrival verification.

Current: [decisions.md](decisions.md#windows-multi-output-foundation).

### 1c. Portable per-domain Sessions ownership (moved 2026-10-24)

> - The portable world Engine owns independent per-domain Sessions, outer gaps,
>   seeding, and relocation behind
>   typed events and replies. Do not merge per-domain revisions, fingerprints,
>   divergence, or node identities into one permanent Session.
>   Send/R4 assemble a transient canonical pair only for the synchronous planned-topology commit; no workspace/R4 pending pair state remains.

Current: [decisions.md](decisions.md#logical-workspace-authority).

### 1d. Obsolete D1 framing fragments (moved 2026-10-24)

The D1/D4/D5 invariants stay current in
[decisions.md](decisions.md#engine-architecture-and-convergence). Only these
framings are historical:

>   revision while independent domain sessions/revisions remain independent.

>   backing remains optional observation. Optional future presentation state
>   remains unimplemented.

>   boundary decisions for the migrated KDE path. The portable per-domain core
>   interface remains transitional for Windows/macOS, not a KDE fallback.

### 1e. Monitor-topology parked-adoption wording (moved 2026-10-24)

> - User direction 2026-10-18: core must own monitor topology, retaining FULL
>   monitor positions in world state and deriving output adjacency itself.
>   Adapters supply geometry in complete world reports, including initialization
>   and updates. Windows full-world adoption stays parked pending this shared
>   delivery; Windows will not build its own adjacency mapping. Selection-policy
>   alignment is not silently decided by ownership extraction.
>   [Offline delivery and open questions](changes/archive/shared-core-owned-output-topology.md).

Current: [decisions.md](decisions.md#engine-architecture-and-convergence).

### 1f. KDE legacy-route transitional sentence (moved 2026-10-24)

>   production uses only DiscoverWorld/DescribeWorld plus float-intent persistence. Portable per-domain core APIs remain until Windows/macOS
>   complete world adoption. [Removal evidence](changes/kde-legacy-route-removal.md).

Current: [decisions.md](decisions.md#engine-architecture-and-convergence).

## 2. Pre-v2 Windows ActiveGroup group-underlay query (historical)

Current (Windows, v2 2026-10-24):
[decisions.md](decisions.md#visuals-border-underlay-and-grouping). The KDE
`SetGroupHighlight` effect bridge is live in the unported KDE adapter and
stays current in
[decisions.md](decisions.md#visuals-border-underlay-and-grouping), not
archived here.

### 2a. Windows ActiveGroup group-underlay query (moved 2026-10-24)

> - Query the focused tiled window's immediate-parent group through Engine
>   `ActiveGroup` and the shared resolver/description, using the actual eligible
>   managed foreground. Render the union of Engine-projected source rectangles,
>   expanded by gap, border width and resolved extension, even while the live
>   dragged frame moves. Hide unavailable/root-leaf/floating, minimised, maximised
>   and fullscreen subjects. Existing pending/drag-residue gates fail closed;
>   Win32 interactive movement pauses reconciliation without adding core residue.
> - Dogfood correction delivered offline 2026-10-11: ActiveGroup uses the same
>   minimum-hint projector as ordinary plans; Windows carries retained advisory
>   hints only, never native/client rectangles. Existing hintless callers and
>   wire schema are preserved. Native nested-layout check pending
>   ([record](changes/archive/windows-group-underlay-geometry.md)).

## 3. Superseded KDE workspace history (moved 2026-10-24)

Superseded by the 2026-10-24 separate-view support and aligned startup
adoption. Current: [decisions.md](decisions.md#workspaces),
[decisions.md](decisions.md#workspace-vocabulary-2026-10-24).

### 3a. Superseded mode-exposure restriction

>     for both separate-view modes: `workspaceMode` exposed only
>     `per-output-local` and `shared`.

Current: [decisions.md](decisions.md#workspaces) (restriction superseded;
the protection and hotplug/focus rules around it stay active).

### 3b. Old per-output-local startup conversion

> - Historical KDE per-output-local startup conversion (2026-10-18),
>   **superseded by aligned startup adoption**. The ownership/splitting and
>   startup-primary rules below no longer apply to KDE aligned mode; exact
>   creation attribution and friendly-name non-identity still apply:
>   - A native desktop initially shown on multiple outputs has one owner: the
>     startup primary. Every other showing output receives its own fresh backing
>     desktop; its non-sticky windows on the shared desktop change membership
>     without changing output or floating/fullscreen/maximized state. Minimized
>     non-sticky windows convert without being restored; sticky windows, including
>     minimized sticky windows, are exempt.
>   - A different pre-existing native current on a secondary output belongs to
>     that output. One ring per native desktop; visible-current evidence is not
>     additional ownership. Core owns conversion and mapping policy; KDE executes
>     membership/view changes only after fresh backing attribution. Incomplete or
>     unreadable startup reports do not establish ownership or authorize conversion.
>   - Create desktops with the unique logical token for exact-name attribution,
>     then rename only the proven bound native to a friendly KDE-style name.
>     Tentative presentation: `Desktop N`, using the one-based native position at
>     proof time. Names need not remain unique after retirement/reordering and are
>     never identities; retained native-ID proof survives rename.
>   - Implementation uses the reported startup-primary fact (KDE currently mirrors
>     `activeScreen`, falling back to output order), latched by the first complete
>     conversion report. Physical-primary versus focused-output startup semantics
>     remain a separate discriminator, not a change to the topology carrier.
>   - Delivered with native KWin 6.7.5 shared/distinct-current, state-preservation,
>     independent switching, two-way output-send and normal controller-restart
>     evidence. [Outcome](changes/archive/kde-startup-desktop-ownership.md).

## 4. Completed owner-idle coverage probe (historical)

Current outcome: [decisions.md](decisions.md#windows-owner-threading).
Probe narrative moved 2026-10-24:

> - User 2026-10-18 selected a controlled native-event coverage probe before 0.1,
>   replacing visible-pager 2 s recovery only if coverage proved complete. Probe
>   completed with completeness unproven: the tested topmost burial emitted a
>   desktop-parent reorder outside current subscriptions, not OS silence; broader
>   visibility/reorder, self-feedback and shell/display coverage remain unproven.

## 5. Historical stage sequencing (tentative, historical only)

Stage 0-5 sequencing in the rearchitecture record was tentative planning
order, never product policy, and stays tentative-historical only. It is not
promoted by the 2026-10-24 close-out. See
[staged plan](changes/archive/core-world-model-rearchitecture.md#staged-plan-and-dependencies)
and the [deferred-work list](changes/archive/core-world-model-rearchitecture.md#deferred-work-tentative-decisions-and-known-gaps).
