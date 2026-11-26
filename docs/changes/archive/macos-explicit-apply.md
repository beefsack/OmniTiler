# macOS explicit Apply verification

**Outcome:** Startup Apply was already removed by `eda5147d`. The pre-port `tile.rs:2091-2127` citation is stale. No runtime change or duplicate startup regression was needed.

- Startup uses the preset to select intercepted chords (`crates/tiler-macos/src/tile.rs:2193-2225`), then starts the runtime (`tile.rs:2427-2436`). The existing `startup_performs_no_desktop_holder_mutation` source regression (`tile.rs:4852-4877`) checks that production tile code contains no startup shortcut-store access and retains the send/readiness path.
- Production Apply is reached through the explicit `spaces-desktop-apply` dispatch (`crates/tiler-macos/src/main.rs:882-890`, `desktop_shortcuts.rs:1387-1414`). `spaces_desktop_commands_parse_with_explicit_intent` (`cli_tests.rs:2233-2258`) distinguishes Apply/Revert from read-only Status; `apply_publishes_preimage_then_enables_with_readback` (`desktop_shortcuts_tests.rs:159`) checks the write sequence with a fake store.
- Managed sends read the holder for the pinned target after refreshing the row and refuse an unreadable or unsuitable holder before posting (`workspace_send_managed.rs:777-843`, `desktop_shortcuts.rs:438-458`). `managed_holder_display_and_sid_refusals_post_nothing` (`workspace_send_managed_tests.rs:985-1042`) checks refusal and zero posts, including disabled, unreadable and customized holders, with a fake backend.
- Tile help describes explicit Apply, and the obsolete test-only startup ensure path has been removed.

**Offline checks:** 3 focused tests passed; the full macOS library suite passed
1,725 with 3 ignored, including the explicit-command parser check among the
passes. Workspace formatting and strict Clippy passed. The integration run
timed out without a final result.

**Native limits:** No live run was performed. Per-drain holder refusal on a real desktop and first-run setup cost remain unverified; the offline checks establish neither native chord delivery nor workspace-action readiness.
