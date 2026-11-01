# World contract and v4 target

Current ownership is recorded below. Phase 0 will complete the full v4
contract here and update v4 ownership.

## Current contract ownership

| Decision or fact | Owner | Basis |
| --- | --- | --- |
| Logical workspaces, membership, ordering and lifecycle | Core | [Section 1](world-contract-v3.md#1-one-core-model-and-policy) |
| Native observation and truthful fact capture | Adapter | [Section 2](world-contract-v3.md#2-report-complete-normalized-facts-every-time), [`WorldReport`](../../crates/tiler-core/src/world_report.rs#L1005-L1035) |
| Native fact normalization and projection | Adapter | [Classification ingress](world-contract-v3.md#classification-ingress-and-remote-transport-rough-edge), [Windows](../../crates/tiler-windows/src/tiling/classification.rs#L194-L215), [KDE](../../crates/omnitiler/src/kde_raw.rs#L426-L437), [macOS](../../crates/tiler-macos/src/classifier.rs#L314-L386) |
| Shared classification and policy | Core | [Classification ingress](world-contract-v3.md#classification-ingress-and-remote-transport-rough-edge), [`SafetyFacts`](../../crates/tiler-core/src/window_policy.rs#L46-L58), [`classify_geometry`/`classify_focus`](../../crates/tiler-core/src/window_policy.rs#L123-L173), [`classify_membership`/`classify_retained_membership`](../../crates/tiler-core/src/window_policy.rs#L187-L230) |
| Native authority and limitations | Constrained by host | [Section 7](world-contract-v3.md#7-host-limits-coordinates-dpi-and-gaming), [Section 6](world-contract-v3.md#6-lifetime-freshness-and-scope-vetoes) |
| Authorized writes and mechanical readback | Adapter | [Section 4](world-contract-v3.md#4-desired-world-state-what-the-reply-authorizes), [Section 5](world-contract-v3.md#5-raw-attempts-and-settlement) |
| Completion, settlement and policy verdicts | Core | [Section 5](world-contract-v3.md#5-raw-attempts-and-settlement) |
| Capabilities and settings conveyed per report | Adapter reports; core consumes | [`WorldReport`](../../crates/tiler-core/src/world_report.rs#L1010-L1218) |
| Presentation eligibility and published shapes | Core | [Section 8](world-contract-v3.md#8-border-underlay-drop-preview-and-pager) |
| Native rendering and immediate safety veto | Adapter and host | [Section 8](world-contract-v3.md#8-border-underlay-drop-preview-and-pager) |

## Approved, not yet implemented (v4)

- [Facts versus classification](../decisions/contract.md#facts-versus-classification): v3 reports projected `WindowFact` ([source](../../crates/tiler-core/src/world_report.rs#L683-L721)); raw `SafetyFacts` are not on the wire ([decision](../decisions/contract.md#facts-versus-classification)).
- [Host capabilities and settings](../decisions/contract.md#host-capabilities-and-settings): v3 carries report-level capability flags/settings ([source](../../crates/tiler-core/src/world_report.rs#L1043-L1100)); approved shape uses a connection handshake and revisioned settings.
- [Intent ownership and persistence](../decisions/contract.md#intent-ownership-and-persistence): v3 carries `observed_workspace`, native membership, and retained presentation frame snapshots ([source](../../crates/tiler-core/src/world_report.rs#L582-L598)); the approved rule puts intent and last-known frame in core and returns opaque host storage unchanged.
- [Send source fallback](../decisions/contract.md#send-source-fallback): v3 `SendSource` has `HomeThenAction` and `HomeThenCachedLive` ([source](../../crates/tiler-core/src/world_report.rs#L831-L844)); v4 retains one action-context chain.
- [Primary output](../decisions/contract.md#primary-output): v3 `ScreenReport.primary` is a boolean ([source](../../crates/tiler-core/src/world_report.rs#L664-L681)); v4 uses one optional primary-output id.
- [Command outcome in reply](../decisions/contract.md#command-outcome-in-reply): v3 has no `Rejected` `CommandResult` ([source](../../crates/tiler-core/src/world_report.rs#L1361-L1442)) and protocol errors are rejected bodies ([codec](../../crates/tiler-protocol/src/world_protocol.rs#L193-L248)); v4 makes rejection an outcome on an accepted reply.
- [Decorations](../decisions/contract.md#decorations): v3 presentation ingress/reply use current typed structures ([ingress](../../crates/tiler-core/src/world_report.rs#L600-L652), [reply](../../crates/tiler-core/src/world_presentation.rs#L111-L127)); v4 uses named decoration fields and one shared revision rule.
- [Size units and scaling (R10)](../decisions/contract.md#size-units-and-scaling-r10): v3 screen facts have no scale ([source](../../crates/tiler-core/src/world_report.rs#L664-L681)); v4 adds per-screen scale and logical-pixel settings.
- [Output identity (R11)](../decisions/contract.md#output-identity-r11): v3 screen facts have no manufacturer/model/serial identity or duplicate fallback ([source](../../crates/tiler-core/src/world_report.rs#L664-L681)); v4 adds stable identity and connector fallback.
- [Native writes difficult to undo (R12)](../decisions/contract.md#native-writes-difficult-to-undo-r12): v3 capability list has no per-window suppression advertisement ([source](../../crates/tiler-core/src/world_report.rs#L1043-L1059)); v4 gates suppression on recoverable preimage storage.
- [Window-state commands (R14)](../decisions/contract.md#window-state-commands-r14): v3 has boolean fullscreen facts and no fullscreen/sticky commands ([source](../../crates/tiler-core/src/world_report.rs#L683-L721), [commands](../../crates/tiler-core/src/world_report.rs#L846-L1003)); v4 adds ownership/exitability facts and shared commands.
- [Scripting command surface (R18)](../decisions/contract.md#scripting-command-surface-r18): v3 world/core command vocabularies are defined here ([world commands](../../crates/tiler-core/src/world_report.rs#L846-L1003), [core command codec](../../crates/tiler-protocol/src/world_protocol.rs#L1224-L1387)); their presence does not establish each platform's external route.
- [Anchored border (R20)](../decisions/contract.md#anchored-border-r20): v3 presentation carries absolute shapes ([source](../../crates/tiler-core/src/world_presentation.rs#L1-L145)); v4 uses anchored border descriptors applied on native events.

## Contract authority rule

The v3 contract as implemented is authoritative now. Each approved change
becomes authoritative only at one coordinated implementation cutover across
core and all adapters. There are no dual message shapes: until that cutover,
the v3 shape governs; at cutover, the coordinated v4 shape replaces it.
The [approved v4 pointers](#approved-not-yet-implemented-v4) record target
mismatches, not interim authority.
See the [authority decision](../decisions/contract.md#contract-authority-and-v4-migration).

Current v3 implementation details are in [world-contract-v3.md](world-contract-v3.md).
