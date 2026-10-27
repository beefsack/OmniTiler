# Empty-workspace focus across outputs: reference evidence

## Goal and scope

Gather pinned Tier 1 source evidence for the existing
[minimal action sequence](../../spec/reference-outcomes.md#empty-workspace-focus-across-outputs):
keep B visible on R, focus A with the cursor on L, select an empty workspace
on L, then immediately send a window to another workspace. Record the focused
output, keyboard target and send subject separately.

Non-goals: select a focus policy, change product code, or claim runtime proof.
Unsupported source outcomes remain TBD for later user testing.

## Acceptance and approach

- Trace COSMIC, Hyprland, i3 and sway in the existing sibling sources at the
  matrix pins; cite file and line ranges for switch, empty focus and send.
- Preserve the existing discriminator and physical observation; distinguish
  internal workspace focus from a client keyboard target.
- Independently recheck every filled cell against its pinned citations.
- Update the backlog to evidence gathered, policy awaiting user decision.
- Verify the documentation diff and archive this record when complete.

## Accepted evidence and outcome (2026-10-28)

- COSMIC at `3d55cba0`: active output remains L. Focus repair clears the
  keyboard target and focused output; a subsequent send then has no subject.
  Repair runs through a 150ms-throttled refresh, not synchronously with the
  numbered switch. The exact immediate pre-refresh send result remains TBD;
  stale A as keyboard target does not prove a transfer of A. The row explicitly
  distinguishes this uncertainty from post-refresh steady state (`EW-C`).
- Hyprland at `19fb395d`: same-monitor empty arrival clears focused window
  and client keyboard focus; send returns without a subject (`EW-H`).
- i3 at `903bcd51`: empty arrival focuses L's workspace node, not a managed
  client; send rejects the empty workspace (`EW-I`).
- sway at `1652c54b`: empty arrival focuses L's workspace node, clearing
  client keyboard focus; send rejects the empty workspace (`EW-S`).
- Each filled cell and its pinned citation chain was independently rechecked,
  including native keyboard-focus clearing and send-subject resolution. The
  COSMIC timing qualification corrects the initial steady-state inference.
- Hyprland/i3/sway leave B on R untouched in the inspected paths. This differs
  from the recorded Windows physical observation, but contradicts no existing
  OmniTiler cell: empty-workspace arrival across outputs remains undecided.
- Matrix preserves the original minimal row and physical observation; backlog
  now records evidence gathered and the outstanding user policy decision.
- Documentation-only change. No runtime test, product change, new policy or
  tentative product decision. COSMIC immediate timing remains for user testing.

## Verification and next action

- Pinned source inspection and independent citation recheck complete; exact
  file:line ranges and full pins live at the point of use in the matrix.
- Documentation whitespace and diff checks complete; no code tests required.
- Next: user chooses the desired policy from this evidence, with the COSMIC
  immediate pre-refresh case explicitly unresolved.
