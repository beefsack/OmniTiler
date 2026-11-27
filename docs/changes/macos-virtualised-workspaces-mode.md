# macOS two-mode workspaces: native Spaces and virtualised mode

- Status: mode count, release timing and gated fresh-install default approved
  2026-11-27. The [release decision](../decisions/platforms.md#macos-01-release-scope)
  supersedes the native-default/virtual-opt-in recommendation in the original
  options. Mode changes, migration and adapter split remain design questions.
- Evidence: External Review: macOS Workspace Modes for OmniTiler, 2026-11-27.
  `rift v0.8.6` provides watchdog and overlap-scoring prior art, `OmniWM v0.8.0`
  native Space/fullscreen exclusion, `HyprMac v0.29.0` the outermost-corner
  fallback, and `Defi v0.11.0` evidence for the empty-workspace focus hazard.
  Tagged citations are source evidence, not OmniTiler native acceptance.

## Terms and rejected end states

- **Native mode.** Workspace membership stays in host Spaces. Rows target observed
  Space SIDs; windows remain native members and move through the existing guarded
  Space routes.
- **Virtualised mode.** Tiler-owned logical rows over one native Space per output;
  inactive members are parked by AX frame writes. Workspace membership is
  tiler-owned; a normal virtual switch does not write native Space membership.

"Configurable" means one mode at a time, not a per-window mixture.

- **A, native-only (ruled out as an end state):** no selectable virtual mode.
- **C, native plus managed pinning only (ruled out as an end state):** serially re-moving a pin is neither native all-Spaces stickiness nor a virtual mode.
- **SIP-off native tier (permanently rejected):** Dock injection violates the
  [anti-cheat rule](../decisions/gaming.md#anti-cheat-mechanism-limits) and is
  incompatible with notarized Developer ID distribution.

Native limitations to document include dependence on guarded native routes
and no verified SIP-on foreign-window sticky route. The
[native 0.1 scope](../decisions/macos.md#managed-native-spaces-input-and-transfers)
includes multiple displays, with individual actions refusing until proven.
Dedicated native fullscreen Spaces remain host-owned.

## Decision 1: which mode is the default

| Configuration | First-run safety | Stranding and rescue | Mission Control expectation | Gate effect |
| --- | --- | --- | --- | --- |
| **Native default, virtual opt-in** (superseded for post-0.1 fresh installs) | New users see existing native behavior and SIP-on routes | Virtual opt-in still needs its own rescue floor | Mission Control shows real Spaces | No change to native acceptance |
| **Virtual default, native opt-in** (selected after safety gates) | Parking is default only after recovery and safety proof | Watchdog and durable pre-park journal recover from abrupt owner loss | Mission Control shows underlying native Spaces, not virtual rows | Requires separate virtual acceptance; does not prove native sticky |

Before the default flips, prove out-of-process restoration after `SIGKILL`
from a durable pre-park frame journal, overlap-minimising edge/corner parking
with a global outermost-corner fallback, and empty-workspace focus fencing.
Keep “Displays have separate Spaces” on; observe native Space topology and
fullscreen read-only, excluding off-Space and fullscreen windows from virtual
writes. Native Spaces remain the sole 0.1 mode. The later default applies to
fresh installs; existing-install migration is not decided here.

## Decision 2: ship virtual mode in 0.1 or after it

| Timing | Cost and test burden | Sticky 6/32, indicator, M8, scopes |
| --- | --- | --- |
| **After 0.1** (selected) | Keeps 0.1 on the native acceptance matrix; virtual parking, reveal, rescue and per-app guards require later proof | Native sticky is a documented limitation; prove virtual output-local sticky, selection, recovery and scope independently |
| **In 0.1** (rejected) | Would add two-mode recovery, geometry, fullscreen, shell and input proofs to the release gate | Would not prove native acceptance |

## Decision 3: restart/per-session mode or live switch

| Approach | What happens | What must migrate | Risk |
| --- | --- | --- | --- |
| **Restart/per-session selection** (recommended first) | Mode is saved; applying a change requires a restart or an explicit session boundary. Native mode re-adopts Spaces; virtual mode starts rows clean or from a saved intent | Workspace identity and frames are re-established by the selected mode; parked state does not cross the boundary | Lowest risk: no half-parked live conversion; users see a restart-required state |
| **Live switch** | Running owner converts membership in place: native Spaces to logical rows and back, parking/unparking along the way | Logical row IDs, native SIDs, output association, frames, focus, pins and indicator projection | A mid-switch failure can leave windows parked or invisible; needs a visible checkpoint, rollback and fault tests |

A restart boundary is safer only if switching out of virtual mode first
verifies that every parked window has been revealed or safely rescued; hold the
change if it cannot. Carry mode-independent preferences and validated
same-lifetime intent only; do not implicitly carry native SIDs, logical rows,
selection/history, or pins across modes. Start from observed windows and the
selected mode's authority. Live conversion would need an explicit mapping and
rollback that does not modify unrelated native Spaces or resurrect stale frames.

## Decision 4: common adapter or forked adapter

| Approach | Shared | Forked/changed | Assessment |
| --- | --- | --- | --- |
| **Common macOS adapter with a mode strategy** (recommended) | AX window identity and attribute observation, app/PID and Space classification, native fullscreen/minimized guards, frame read/write primitives, activation and animation suppression, indicator host, owner-loss supervisor | Membership policy, parking/unparking controller, persistence/restart intent, mode-specific selection and rescue | One observation and guard stack; branches stay at the mode seam. More up-front interface work, less duplicated proof |
| **Forked native and virtual adapters** | Little beyond shared crates and UI primitives | Two observation stacks, two guard sets, two indicator integrations, two test matrices | Isolation is clearer, but divergence in AX behavior and recovery is likely; not recommended without evidence that the common seam is unsafe |

Most AX plumbing can be shared beyond the core: window identity/attributes,
app/PID and Space classification, fullscreen/minimized guards, frame writes,
activation/animation suppression and owner-loss supervision are mode-neutral.
New in the adapter: a parking controller, per-window visibility state, pin/lease
policy, crash ledger and rescue helper, and per-output reanchor/hotplug
reconciliation. Core changes are membership, visibility and switch intent, not
a second platform abstraction.

The existing macOS [restart-intent store](../decisions/persistence.md#macos-restart-intent-store)
provides a session/lifetime identity and atomic-write pattern, not a frame or
visibility journal. Reuse owner-loss plumbing if it can operate independently
of the dead owner; persist verified identity, pre-park frame and output before
parking. An independent rescue must revalidate identity, parking state and
topology before writing; on changed topology, safe on-screen placement is
rescue, not exact restore. Do not act on arbitrary off-screen windows.

## Shared menu-bar indicator

One shared menu-bar indicator, each display's group separated, not a physical
carrier per output, is the selected presentation. Hide under ambiguous
output-to-workspace attribution, not merely because multiple displays exist.
The physical grouped-item check is separate from single-display MX6.

The modes cannot share one target token:

- **Native rows** carry the observed immutable SID plus UI generation and topology
  revision through click admission/native drain
  (`docs/changes/macos-workspace-indicator.md:68-71`). A click resolves a display
  and exact SID, then uses a capability-checked selector. Grouping in UI is
  not multi-display acceptance.
- **Virtual rows** carry a logical workspace ID plus output. A click selects the
  row and parks/unparks; no MC AX or Dock swipe for inactive rows. Pins and native
  fullscreen may still need SID work, so the SID is not globally irrelevant.

A shared indicator needs a target enum (native SID or logical row), stale-target
refusal, mode-aware occupancy and a display separator; no physical acceptance is
claimed.

## Reference mechanisms

| Reference | Parking and restore | Crash behavior | Shell/fullscreen limit |
| --- | --- | --- | --- |
| AeroSpace v0.21.3-Beta | Bottom-corner AX parking; floating position saved proportionally and clamped, tiled layout re-applied on return (`AeroSpace v0.21.3-Beta:Sources/AppBundle/tree/MacWindow.swift:123-183`) | Quit recenters windows, not exact restore; upstream documents manual sliver rescue after a bad crash (`AeroSpace v0.21.3-Beta:docs/guide.adoc:450-452`) | Real Spaces still appear in Mission Control, but virtual rows don't; parked thumbnails are tiny (`AeroSpace v0.21.3-Beta:docs/guide.adoc:476-487`) |
| rift v0.8.6 | SIP-on AX parking, virtual rows under each native Space; 1 px reveal and overlap scoring (`rift v0.8.6:README.md:40,46`, `rift v0.8.6:src/model/virtual_workspace.rs:193-221`, `rift v0.8.6:src/model/hidden_window_placement.rs:81-149`) | Separate recovery child; EOF on daemon exit triggers a bounded cascade rescue, not exact restore (`rift v0.8.6:src/sys/window_recovery.rs:15-124`) | Its own Mission Control-style overview; native fullscreen record retains the virtual origin (`rift v0.8.6:README.md:34-37`, `rift v0.8.6:src/model/window_store.rs:95-113`) |
| OmniWM v0.8.0 | AX/SLS parking with 1 px reveal when enabled; alternate edge on overlap (`OmniWM v0.8.0:Sources/OmniWM/Core/Layout/SideHiding.swift:83-133`) | Graceful stop restores targets and flushes the catalog; no signal/crash handler found, abrupt-death behavior not established (`OmniWM v0.8.0:Sources/OmniWM/Core/Controller/ServiceLifecycleManager+WindowRecovery.swift:7-78`) | Fullscreen excluded from parking/restore (`OmniWM v0.8.0:Sources/OmniWM/Core/Controller/ServiceLifecycleManager+WindowRecovery.swift:22-40`) |
| FlashSpace v4.19.83 | App-level hide; PiP special-cases window parking. Cannot split windows of the same app (`FlashSpace v4.19.83:README.md:51-55,121-136`) | Documented quit hook restores PiP parking, not a general crash guarantee (`FlashSpace v4.19.83:FlashSpace/App/AppDelegate.swift:47-49`) | Virtual app groups aren't native Spaces; no native fullscreen integration established |

AeroSpace's guide calls the visible sliver unavoidable for its own design, not a
proof for every app/scale/arrangement (`AeroSpace v0.21.3-Beta:docs/guide.adoc:450-474`).
It probes both bottom corners for other displays and picks the less obstructed
one; a stacked display can cover both, and side placement is not attempted
(`AeroSpace v0.21.3-Beta:Sources/AppBundle/layout/refresh.swift:155-188`).
rift scores overlap and supports side placement; its geometry tests cover
negative/fractional origins and scale rounding, but the design still reveals
one pixel (`rift v0.8.6:src/model/hidden_window_placement.rs:88-179`).

AX writes are asynchronous and per window. AeroSpace sets size, position, then
size again, disables the app's enhanced UI while writing, and cancels superseded
jobs (`AeroSpace v0.21.3-Beta:Sources/AppBundle/tree/MacApp.swift:150-157,411-435`).
Zoom needs a zero one-pixel offset or it jumps off
(`AeroSpace v0.21.3-Beta:Sources/AppBundle/tree/MacWindow.swift:144-152`). No
surveyed virtual tool demonstrates a per-app refusal/undo retry matrix; other
apps that refuse or undo writes need their own evidence.

SIP-on feasibility is for the **actuator**, not an AX-only end-to-end
guarantee: the AeroSpace move is public AX without Dock injection, but its
inventory joins a private window ID (`AeroSpace v0.21.3-Beta:README.md:123-126`).
Per-window write authority, complete observation, and exact display arrival
still need proof.

**Native comparators.** yabai keeps real Spaces; its direct sticky route is
part of the reduced-SIP Dock scripting addition, not an AX parking precedent
(`yabai v7.1.25:README.md:53-67`, `yabai v7.1.25:doc/yabai.asciidoc:426-428,666-668`).
Amethyst keeps layout per real Space UUID and reflows only user Spaces, not
native fullscreen Spaces (`Amethyst v0.25.1:Amethyst/Managers/ScreenManager.swift:158-173,248-261`).

**Shell and native states.** Virtual rows in all four virtual tools don't get
native Space thumbnails/selection. Native fullscreen still creates real
Spaces. AeroSpace puts minimized, fullscreen and (under its hide setting)
hidden-app windows in separate containers and re-tiles/re-floats on return
(`AeroSpace v0.21.3-Beta:Sources/AppBundle/normalizeLayoutReason.swift:24-72`).
It recommends separate Spaces off, which can blank another display during
native fullscreen (`AeroSpace v0.21.3-Beta:docs/guide.adoc:489-533`); that
trade-off isn't a setting change proposed here. Dock/App Exposé and Stage
Manager remain OS surfaces, not virtual-workspace controls. Parking only
changes geometry; app hide changes app visibility. Exact listings,
activation of parked windows, and Stage Manager behavior are unverified in
each virtual mode; don't promise isolation or native integration.

## Gates, sticky and scopes

| Area | Native mode | Virtual mode |
| --- | --- | --- |
| Sticky (6/32) | Documented SIP-on foreign per-window limitation; the [bounded probe](macos-0.1-native-spaces-prior-art.md#bounded-signed-fixture-probe-2026-11-27) stopped before a write | Output-local sticky presentation, not native all-Spaces membership |
| Indicator | Exact native SID per click and capability-checked selector | Logical row plus output; no SID selection for an inactive virtual row |
| Switch latency/animation | Host Space animation; MX1 recorded an 88-98 ms native follow, not generalized and not a virtual comparison | Parking frame writes and activation replace the animation; latency, focus and view settlement unquantified |
| Fullscreen (MX5) | Native fullscreen Space remains host-owned | Fence parking while fullscreen; retain logical home and settle windowed return without forcing fullscreen exit |
| M8 cross-output | [M8](v4-migration.md#m1-m9-technical-resolution) needs target-display frame plus native Space membership; provisional `tiledFloatTransfer=false` is not a general move ban | Logical membership is easier to change; display/Space reassociation still needs native arrival proof for that substrate, not a position-setter return |
| Hidden members (33); selection/sends (14/15/42/43, MX1) | Off-Space windows receive no background geometry writes; membership and selection are native | Park once, keep background layout logical, and verify hide/reveal and arrival before focus. Parked isn't minimized, closed or reliably invisible to shell UI. The parking actuator and logical selection require gate re-mapping, not reuse of the original proofs |

Virtualised mode will provide output-local sticky **presentation**: exclude
an authorized pin from parking so it stays visible on its home output without
a per-switch move. Preserving pin intent over restart would need a
lifetime-scoped record. The pin does not follow to another output or native
Space and is not native sticky proof.

[Workspace scope](../decisions/workspaces.md#workspace-scope-vocabulary) is
separate from the actuator. `per-output-local` uses independent rings and
views; `global-unique` additionally needs cross-display selection/swap;
`shared` links all outputs to one logical current and must settle reveal on
each. None is selected for virtual mode here. Windows is the closer policy
model (tiler-owned rows) but hides with `SW_HIDE` and reveals without
activation; macOS frame parking isn't equivalent. KDE's Planner owns policy
while KWin owns native desktops and the shared roster; like current native
macOS mode, its backing/pager proof isn't a virtual visibility receipt.

## Recommendation (provisional, not decided)

The restart/per-session and common-adapter recommendations are provisional:

1. **Restart/per-session switching first**; live switch only with migration,
   rollback and fault tests.
2. **One common macOS adapter with a mode seam**, sharing AX/observation/guards
   and branching at membership, parking, persistence and selection.

The approved default gates are listed under [Decision 1](#decision-1-which-mode-is-the-default).
Define separate virtual proof for geometry, shell and per-app behavior and a
pin/lease contract that distinguishes output-local presentation from native carry.

## Material unknowns

- No surveyed tool demonstrates exact frame + membership + focus + view restore
  after crash; rift demonstrates rescue in source, not exact restoration.
- Per-app AX refusal/undo/popup churn and activation-time frame fights are not
  covered by a matrix.
- Mission Control/App Exposé, Dock activation, empty-view keys, Stage Manager and
  game fullscreen/timing are unmeasured; mixed-scale/mirrored/stacked displays,
  hotplug, sleep/wake and Spaces auto-rearrange remain open, and the one-pixel
  reveal is expected, not universally proven.
- Physical acceptance of the shared multi-display indicator is pending.
  There is no positive native SIP-on sticky evidence; virtual sticky is not
  evidence for it.

## Chosen tags

Tags fetched 2026-11-27; checkouts refreshed fast-forward only. No stable
AeroSpace release exists upstream, so `v0.21.3-Beta` is a pre-release fallback.

| Reference | Tag | Kind | Commit |
| --- | --- | --- | --- |
| AeroSpace | `v0.21.3-Beta` | pre-release fallback (published 2026-07-16 as prerelease) | `d56e1637c3a1ed660d0cadd7534e94fb3218d1c3` |
| yabai | `v7.1.25` | stable | `02de172f8706c3e2eaa679478897d7346e82b564` |
| Amethyst | `v0.25.1` | stable | `0d20cf14a97486129dbf073c397bbf1b0e316baa` |
| FlashSpace | `v4.19.83` | stable | `6d66f06d4a3ec3142e8cf746b4f201f9472afac6` |
| rift | `v0.8.6` | stable | `f13c2513ed96c081e727b6b7d1294b3bdc368f93` |
| OmniWM | `v0.8.0` | stable | `bf118cca1449468397dee3662acbba5544657545` |

## Local evidence

[Gate proposal](macos-0.1-gate-proposal.md), [product scope audit](macos-0.1-product-scope-audit.md),
[indicator record](macos-workspace-indicator.md), [workspace model review](../research/macos-port/workspace-model-review.md),
[persistence](../decisions/persistence.md), [Windows hidden-workspace research](../research/windows-port/alt-tab-hidden-workspaces.md),
[M8 migration](v4-migration.md).
