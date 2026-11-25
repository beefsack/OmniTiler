# macOS Current-Workspace Tiling Toggle

- **Status:** Complete (2026-11-25), offline verification only.
- **Source:** [macOS v4 open-decisions review](../macos-v4-open-decisions-review.md) Check 20 (0.1 Must-Pass) — `ToggleWorkspaceTiling` / `Ctrl+Y` / menu-bar toggle equivalent.
- **Boundary:** No native, fixture, or AppKit-visual leg. `docs/backlog.md` is owned by the orchestrator; this note carries the slice's status wording.

## Goal

Deliver the current-workspace tiling toggle on macOS end to end offline:

- **Shortcut:** configured modifier role + `Y` (default `Ctrl+Y`), one toggle per discrete key down (held auto-repeat suppressed like orientation/maximize), matching Windows `Win+Y` / KDE `Meta+Y` intent.
- **Equivalent:** a checked `Tiled` item at the top of the workspace-indicator dropdown menu. Clicking toggles the current workspace; the checkmark reflects the current workspace's core `TilingMode` from the coherent latest reply and refreshes on the toggle reply itself.
- **Core:** the existing neutral `WorldCommand::ToggleWorkspaceTiling { output, workspace }` flips the retained core mode. No core, protocol, or wire change.

## Scope

- `InputSemanticAction::ToggleWorkspaceTiling`, the Ctrl-role+Y catalogue chord, tap repeat suppression, engine dispatch with honest refusal.
- Domain resolution: the focused window's exact current homed domain; otherwise exactly one readable current domain; otherwise refuse with no mutation.
- Indicator projection tiling state, the menu toggle item (menu only — workspace rows stay selects, the segment bar is unchanged), and its click route through the guarded input lane.
- A monotonic indicator revision so a toggle reply repaints the checkmark before the next topology/observation tick.
- Focused offline regressions for every checkable behaviour.

Non-goals: persisted future-default seeding (no macOS Settings/default-tiled surface yet — Package 5); native/AppKit visual proof, fixture legs, live journeys, per-window float/sticky, native maximize/fullscreen overlays; any adapter-side tiling policy, cached mode on the command, or new dependency.

## Approach

- The toggle is a basic catalogue chord. It needs no focused-window subject, so `world_command_map::toggle_workspace_tiling_command` resolves exactly one `(output, workspace)`: with a focused window, its retained home (`RetainedCore::window_domains`) must also appear in the live `NativeContext` with matching output and workspace and be `current && readable`, otherwise it refuses; without a focused window only a unique `current && readable` context domain resolves. No live context at all refuses before dispatch.
- Native domains are native-keyed (`space-N`) while core names logical workspaces; the command's workspace is translated only through the applied reply's published binding rows (`pager_bindings_for` → `Shared::last_pager_bindings`), the adapter's single native→logical table. No binding row means no logical workspace to toggle yet: honest refusal, never a native id or a substituted domain.
- A focused window NEVER falls back to the current-domain branch and never borrows a stale home: the retained home must be present in the live context as `current && readable` (no context ⇒ refusal), because flipping a hidden/Space-switched-away workspace would be a mutation the user did not ask for. The unique current-domain fallback applies only with no focused window.
- The command is neutral: mode resolves in core at execution, so racing toggles compose and the adapter never carries a cached mode.
- The indicator menu item is a menu-only extra row with sentinel tag `0` and its own bounded channel; it logs a render-time UI generation and submits `ToggleWorkspaceTiling` through `EngineJob::InputAction` (`RuntimeHandle::submit_input_action`). It never consults a position, SID, or ordinal.
- The checkmark reads `WorkspaceDesire::tiling_mode` of the current pager row in `project_core_indicator`; `None` (unproven) disables the item instead of guessing. A monotonic `Shared::indicator_revision` bumps when applied-reply desires or the pager presentation change, giving `IndicatorDriver::needs_tick_with_revision` an edge on the toggle reply itself.
- `row_titles()` filters with `is_workspace_row_tag` (tag > 0), so the prepended toggle never pollutes the workspace-row readback used by the live proof and fixtures.

## Units

1. Keyboard path: action, chord, repeat suppression, mapping/refusal, dispatch, reply token, focused regressions.
2. Indicator path: projection tiling, menu item + sentinel route, guarded-lane submit, indicator revision + driver trigger, focused regressions.
3. Change note, verification gate, staging.

## Accepted evidence

Offline gate at the final tree (2026-11-25):

- `cargo fmt --all -- --check` — clean.
- `cargo test --locked -p tiler-macos --lib` — **1,716 passed / 0 failed / 3 ignored** (pre-change baseline 1,699; seventeen new regressions).
- `cargo test --locked -p tiler-macos --test integration` — **152 passed / 0 failed**.
- `cargo clippy --locked -p tiler-core -p tiler-protocol -p tiler-macos --all-targets -- -D warnings` — clean.

Focused regressions:

- Shortcut/armed: `input_policy::tests::workspace_tiling_toggle_ctrl_y_stays_armed_under_compatible`; `native_input_tests::workspace_tiling_toggle_catalogue_is_one_bare_ctrl_role_y_chord` (exactly one Ctrl-role+Y, no extra arms); catalogue count assertions updated across crate and integration targets (36 basic; 55/65 product totals; `RuntimeConfig` validation).
- Mapping: `world_runtime_tests::workspace_tiling_toggle_resolver_prefers_the_focused_homed_domain` (live context corroborates the home as current+readable; a binding row for another domain does not leak), `..._refuses_a_focused_home_without_context`, `..._refuses_a_hidden_focused_home` (retained home present but not current ⇒ no toggle), `..._refuses_an_unbound_focused_home` (current home, no binding row ⇒ translation refusal), `..._falls_back_to_the_unique_current_readable_domain`, `..._refuses_zero_current_readable_domains`, `..._refuses_ambiguous_current_domains`, `..._refuses_without_context_or_focus`, plus the extended `operation_commands_cover_every_supported_tiling_action`.
- Real core: `runtime_thread::trailing_join_tests::workspace_tiling_toggle_real_core_flips_and_restores` — two presses through the genuine `process_job` InputAction route flip the resolved workspace `Tiled` → `Floating` → `Tiled`; the floating leg authorizes no frame writes; two commands forwarded.
- Refusal: `runtime_thread::trailing_join_tests::workspace_tiling_toggle_unresolved_refuses_without_core_contact` — no focused home and no/ambiguous current domain increments `input_refused_no_focus`, emits `toggle-workspace-tiling-unresolved`, and touches no core.
- Indicator: `workspace_indicator::tests::core_projection_carries_current_workspace_tiling_mode`, `unproven_tiling_stays_none_never_guessed`, `projection_tiling_change_advances_driver_generation`, `driver_needs_tick_tracks_indicator_revision`, `driver_counts_only_accepted_toggles`, `toggle_click_forwards_neutral_intent_or_discards_stale`; `world_dispatch::tests::indicator_revision_advances_on_changed_desires_and_holds_on_duplicates` drives the real applied-reply publish block.
- Drain path: `tile::tests::drain_toggle_clicks_counts_one_acceptance_per_click` (real channel + matching generation + accepting submit ⇒ exactly one `toggles()` increment; fails on a double count), `drain_toggle_clicks_counts_refused_submissions_as_neither`, `drain_toggle_clicks_discards_stale_generation_without_submitting` (submit never invoked, one discard).

## Material decisions

- Native→logical workspace translation for this command uses only `Shared::last_pager_bindings` (the applied reply's `native_binding` rows); no adapter-side binding table and no ordinal reinterpretation.
- A focused window with unproven homing refuses rather than borrowing the unique-current-domain fallback, and its retained home must be corroborated by the live context as `current && readable`; a stale/hidden home never toggles the workspace it used to occupy.
- The toggle item is a menu-only control with sentinel tag 0 and a separate bounded channel; it submits neutral intent through the same guarded input lane as a tap and is fenced by the render generation only.
- The checkmark repaint trigger is a monotonic revision over applied-reply desires/pager changes, not a new poll or timer; existing `last_indicator_snapshot()` (4-tuple) and driver `needs_tick`/`apply` wrappers stay stable for tests.
- The drain loop lives in an AppKit-free `drain_toggle_clicks` seam so the exactly-once acceptance counting is regression-tested at the path the live guard uses; no behavioural change and still one `toggle_submitted()` per accepted click (review claim of a double count was checked against source and did not hold).

## Verification limits

- **No native/AppKit visual proof was performed.** The menu item is constructed and rendered by code covered by type checking and pure tests; its actual AppKit appearance, checkmark rendering, and click delivery were not observed. No fixture or live leg ran.
- Behaviour of the real native tap (chord consumption on a live session) is likewise unobserved.

## Known Check-20 remainder

Persisted future-default seeding (`Tiled`/`Floating` default for new workspaces) is not implemented and is not implementable on macOS until the Settings/default-tiled surface exists (Package 5). Check 20 is advanced, not fully closed.
