# Two-output KDE test environment

## Goal and acceptance

- Research a user-operated environment for KDE 0.1 multi-output must-pass
  checks without a physical second display.
- Map the approved live-check inventory to static topology, hotplug,
  mode/scale, mixed-scale, arrangement and sleep/wake requirements.
- Compare nested KWin, virtual/headless Plasma, a full Plasma VM and credible
  alternatives using pinned KWin 6.7.4 source (`8438567a`) and primary docs.
- Recommend the smallest useful route; distinguish synthetic coverage from
  hardware fidelity and identify remaining release-gate decisions.
- Add a launcher and procedure only if existing dependencies and the live-test
  boundary support the complete route; otherwise identify prerequisites.

## Approach

| Unit | Scope | Acceptance evidence |
| --- | --- | --- |
| KWin capability investigation | Gate inventory, nested and virtual backends, full-stack feasibility | Pinned source anchors and explicit unsupported/untested distinctions |
| VM comparison and recommendation | Virtual GPU heads, display events, packaging, dependencies and alternatives | Primary-source comparison and concrete prerequisites |
| Integration | Research, promoted backlog slice, conditional launcher/procedure | Reviewed diff, static checks, accurate pending live evidence |

## Boundaries and verification

- General reference-environment provisioning remains post-0.1.
- Environment research and static verification do not establish live acceptance.
- Preserve the existing nested isolation contract; identify shortcut and session
  fidelity gaps rather than silently changing it.
- Verify links, source anchors, authored ASCII and the final diff.

## Outcome

- Research delivered in [route comparison](../../research/two-output-test-environment.md).
  Full Plasma VM recommended: two connected virtio heads normally, three for
  the two-candidate check. Route selection and provisioning remain pending.
- Pinned KWin source confirms parent-driven nested scale and a dynamic
  nested-output registration gap; virtual/DRM screencast outputs have a
  separate registered create/remove path. None is runtime acceptance.
- Pinned QEMU source confirms explicit connected-head geometry and GTK resize
  events, but tab closure does not unplug. Synthetic hotplug identity,
  physical sleep/wake and native gaming remain explicit fidelity gaps.
- Existing NixOS packaging supplies script/effect/settings, not planner/tray;
  the guest needs their session integration and an exact guest-KWin effect.
  QEMU is the proposed new host tool; no dependency or launcher was added.
- The narrowed environment slice is pre-0.1; the general reference-environments
  item remains parked. The must-pass gate is unchanged.
- Verification: independent source/fidelity review accepted the comparison;
  authored Markdown is ASCII, relative file targets exist, and the display
  device fragment parses as JSON. Static research only; no live runs.
