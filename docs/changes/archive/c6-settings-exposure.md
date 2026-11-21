# C6: Adapter-owned settings exposure

## Scope and design

Remove the portable settings catalog's host exposure flags without changing
which settings are offered, how settings are stored or validated, or the v4
wire shape. Neutral shell-surface reporting and its Win32 classification
residue remain separate work for the Windows port.

The flags describe UI exposure, not a hard limitation core needs to enforce.
[KDE's exporter](../../../crates/tiler-kde-settings-exporter/src/kde_adapter.rs)
already selects visible, schema-only and unsupported fields; the KWin script
already constructs its own revisioned settings snapshot. Protocol and broker
route those values, not a list of fields to advertise to the UI. Remove the
unused metadata rather than inventing a parallel list in the handshake or
coupling UI availability to runtime actuation. Core still owns shared field
semantics and effective settings, including its existing neutral capability
checks. The macOS runtime consumes catalog defaults and submits settings but
does not depend on the removed flags. Windows uses them for its native-only
field descriptors; the Windows port must drop those initializers and the
obsolete assertion/export when it adopts the slimmed descriptor.

## Acceptance

- No platform exposure flags in core and no compatibility aliases.
- Prove KDE's unchanged generated UI/schema and exact runtime snapshot keys.
- Run formatting, clippy and Rust tests for the supported core, protocol and
  daemon; run the raw-world evaluator and the KWin typecheck and tests.
- Keep all frozen fixture families byte-identical.

## Outcome

- Removed the per-field host flags with no replacement or protocol change.
  The KDE exporter still exposes 17 controls, with three unsupported fields
  and one schema-only setting. Its 14 tests and generated-file check passed.
  The KWin snapshot regression pins all 11 runtime keys.
- Package formatting, all-targets clippy with warnings denied, and tests
  for `tiler-core`, `tiler-protocol`, and `omnitiler` passed on a snapshot
  with only the C6 changes, excluding concurrent planner test edits.
  The `raw_world_eval` example test and KWin typecheck and tests passed;
  KWin had 1,477 passing tests and one skipped.
- Frozen `world-wire/v4`, `stage5`, `stage5-v4` and `kde-v4` fixtures match
  `ff6eaddc`.
