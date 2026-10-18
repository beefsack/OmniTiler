# KDE startup desktop ownership and friendly names

## Goal and scope

- Deliver the two 2026-10-18 user decisions: core-owned per-output-local startup
  conversion with one ring per native, and friendly names after exact creation
  attribution. Resolve the two startup-ring reference-outcome TBDs.
- Preserve output and floating/fullscreen/maximized/minimized state; sticky
  clients are exempt. Distinct pre-existing secondary currents belong to their
  shown output. KDE executes policy, not a second adapter partition algorithm.
- Non-goals: whole-workspace migration, physical shortcut delivery, complete
  multi-output inventory, persistent mapping, or a new multi-desktop model.

## Root causes and correction

- Startup adopted every unassigned native onto primary. Publication also emitted
  a foreign-owned current under its showing output; binding feedback could then
  retain it in both rings. Visible-current evidence was mistaken for ownership,
  producing extra workspaces and `cross-domain-mismatch` sends.
- Core claims distinct currents from the first complete readable startup report,
  primary first. An initially shared current stays single-homed; each other
  showing output receives one pending fresh backing token. Only exact binding
  proof enables same-output member/view conversion. Later explicit ownership
  and views remain authoritative.
- Retained mapping wins over stale binding echoes. An exact outstanding
  logical-token/output proof may rehome a newly adopted, unshown native; ordinary
  echoes do not steal existing ownership. Incomplete/unreadable startup evidence
  neither seeds ownership nor consumes conversion. Unowned lifecycle rows are
  omitted under partial evidence, not duplicated provisionally.
- Minimized entries now carry required native `sticky` metadata through core,
  protocol and producers. KDE retains full local minimized carriers for
  same-output membership-only actuation; frame authority remains minimized,
  sticky minimized clients are exempt, and fresh homing proof settles transfers.
  Other-platform edits are compatibility-only. Source-slot restore policy is
  unchanged. Mixed revisions with nonempty old-shape minimized entries refuse
  strictly rather than inventing missing sticky state.
- Creation still uses the unique logical token as native name. Immediate or
  delayed exact-name attribution stores native-ID proof first, then renames only
  that proven desktop while its name still equals the token. Rename failure
  preserves proof and never repeats creation. Later feedback and restart use IDs,
  not friendly names.

## Native API and tentative presentation

- KWin v6.7.5 at `ab7df7ccb7c6af20f4b279cd6220f7cd3d2267d7` exposes writable
  [`VirtualDesktop.name`](https://github.com/KDE/kwin/blob/ab7df7ccb7c6af20f4b279cd6220f7cd3d2267d7/src/virtualdesktops.h#L60)
  through `workspace.desktops`. `setName` emits `nameChanged`; creation is void
  `(position, name)`. Local pinned-source inspection and native readback agree.
- Tentative format: `Desktop N`, using one-based position in the proving native
  enumeration, matching KDE's default-name style. Retirement/reordering can leave
  duplicate friendly names; no identity or attribution relies on uniqueness.
- Startup primary uses the existing reported primary fact (KDE `activeScreen`,
  fallback output order), latched at the first complete conversion report.
  Physical-primary versus focused-output semantics remain a separate TBD row.

## Verification and accepted evidence

- Producer-shaped core startup regressions failed before correction: duplicate
  ownership, missing conversion, delayed-binding duplication and wrong distinct
  secondary homing. Naming regressions failed on immediate/delayed/two-token
  friendly-name expectations. Minimized conversion failed on the missing mover.
- Review found stale-seed stealing, partial-startup ownership and unreadable-domain
  consumption; corrected with focused RED regressions. A final partial-publication
  regression also failed before suppressing unowned fallback rows. Independent
  review verified the safety corrections and minimized actuator.
- The migration golden changed only its obsolete duplicate workspace row/order;
  scope/output/window/transfer/result semantics are unchanged. Supported fixture
  regeneration preserves the selected one-ring oracle.
- Final integrated static gates: Rust workspace **3216/3216**, KWin **1880/1880**,
  codec **24/24** and unsafe guardrail **32/32** pass. Strict workspace/all-target
  Clippy, formatting, typecheck, script build, rebuilt Planner fixture and settings
  descriptor freshness pass after integration. Exporter tests **13/13** pass;
  focused startup regressions **14/14** pass. No fixture changes were needed after
  final integration.

| Native KWin 6.7.5 leg | Accepted outcome |
| --- | --- |
| A: shared current, ordinary clients | Primary retains shared native; secondary receives a uniquely bound fresh backing and spare. One ring/native; no foreign-current duplicate. Four independent numbered/relative transitions pass. First output-send follow direction arrives/focuses without changing views. |
| A: normal controller/Planner restart | Renamed native desktops retain ID-based attribution; no name-driven stealing/recreation. Ordinary empty retirement/spare creation remains selected lifecycle behavior. |
| B: shared current, exceptional states | Ordinary, intentional-floating, fullscreen, maximized and minimized non-sticky clients move backing membership on the same output with state preserved. Sticky and minimized-sticky clients remain unchanged. Inverse output-send follow passes with an owned occupied target. |
| C: distinct pre-existing currents | Each current belongs to its showing output; no conversion moves or current replacement. Eight independent numbered/relative transitions pass; new spare is friendly-named. |

- Each leg captured complete request/reply bodies before enable plus native
  before/after state and direct `per-output-local` mode. No refusal remains on
  the two independently staged output-send follow directions. These are not a
  rapid round-trip, send-stay, migration, physical-input or full-inventory pass.
- Initial A teardown could not restore hidden Firefox frames by direct setter;
  detaching stale tile association alone also did not settle frames. Showing the
  affected desktop on its output, draining native configure, then restoring exact
  frames and original views resolved it. Settled readback, not setter success,
  proves restoration. Later legs used this corrected restoration procedure.
- Final protected frames/modes/tile associations/memberships/outputs/views/focus/
  pointer, shortcuts, effects, output configuration, float-store absence and
  stopped development lifecycle match their respective baselines exactly. Owned
  fixtures are gone. Only documented normal empty-desktop retirement/appending,
  names/order and corresponding default tiling groups remain as product churn.

## Limits and completion

- Multi-desktop non-sticky membership retention and physical-primary selection
  remain explicit reference-outcome discriminators, not inferred acceptance.
- An interrupted creation before proof can retain an unrenamed token desktop;
  restart never guesses its identity. No crash/restart-persistent mapping promise
  is added. Delayed enumeration is covered offline; ordinary restart is live-proven.
- Two selected decisions are promoted to `docs/decisions.md`; broader inventory
  work remains in [the active dual-output record](../kde-dual-output-live.md).
