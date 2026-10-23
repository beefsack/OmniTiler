#![forbid(unsafe_code)]
//! Tiling-only Windows helpers over the world route.
//!
//! Shared policy lives in the core (`tiler_core::window_policy`); this crate
//! keeps only host fact collection and the thin `classify` / `classify_focus`
//! normalization plus its native log vocabulary. Native enumeration,
//! actuation, and the event loop live in [`crate::tiling_sys`] (`cfg(windows)`
//! only). Layout authority is the core world engine: the world sequencer
//! captures complete world reports and applies the accepted desired snapshot
//! verbatim, never adopting observed rectangles into retained state.

use std::collections::HashMap;
use std::path::PathBuf;

use tiler_core::geometry::Rect;

mod classification;
mod cli;
mod float;
mod geometry;
mod reconcile;
mod workspace;

pub use classification::{
    GENERIC_DIALOG_CLASS, OWN_SETTINGS_WINDOW_CLASS, ObservedTargetRef, SkipReason, TokenMap,
    WindowFacts, classify, fingerprint, is_own_settings_window, workspace_dialog_refuses,
};
pub use cli::{
    CaptureOptions, CliOverrides, HideProofOptions, ScopeHostChild, ShortcutProofOptions,
    TileOptions, TileProofOptions, WorkspaceProofOptions, apply_cli_overrides, hosted_child_allows,
    parse_capture_args, parse_gap_value, parse_hide_proof_args, parse_scope_host_child,
    parse_shortcut_proof_args, parse_tile_args, parse_tile_args_from,
    parse_tile_args_from_with_overrides, parse_tile_proof_args, parse_workspace_proof_args,
    scope_allows, scope_exe_basename, tile_options_defaults, verify_hide_proof_argv_consistency,
    verify_proof_argv_consistency, verify_shortcut_proof_argv_consistency,
    verify_workspace_proof_argv_consistency,
};
pub use float::{
    FullscreenToggle, RESTORE_WAKE_MS, StatelessVerdict, authorize_maximize_clear,
    canonical_retained_rect, classify_focus, float_intent_marker_value,
    float_topmost_restore_needed, fullscreen_toggle_decision, inspect_stateless_verdict,
    maximize_clear_refusal, parse_float_intent_marker, parse_sticky_marker,
    parse_tile_override_marker, postclear_fullscreen_now, restore_wake_arm, restore_wake_step,
    retained_overlay_hint, send_flags_stable, stale_fullscreen_retire_eligible,
    sticky_marker_value, tile_override_marker_value, toggle_gate_outcome,
};
pub use geometry::{
    FrameInsets, INNER_GAP, OUTER_GAP, OUTPUT_ID, OWNER_ID, WORKSPACE_ID, hints_from_outer,
    min_hints_from_outer, normalize_max_track, normalize_min_track, normalize_outer_bound,
    overconstrained_effective, tiling_domain_bounds, tiling_domain_bounds_with,
    visible_max_from_outer, visible_min_from_outer,
};
pub use reconcile::{
    AllowEntry, ObservedTarget, allow_match, allowlist_digest, dpi_transition_reouter,
    parse_allowlist,
};
pub use workspace::{
    ChildrenOptions, DIRECTIONAL_REQUEST_FILE, DIRECTIONAL_REQUEST_VERSION, DirectionalOp,
    DirectionalOptions, DirectionalRequest, InspectOptions, MIGRATE_WORKSPACE_REQUEST_FILE,
    MIGRATE_WORKSPACE_REQUEST_VERSION, MigrateWorkspaceOptions, MigrateWorkspaceRequest,
    RESIZE_REQUEST_FILE, RESIZE_REQUEST_VERSION, ResizeOptions, ResizeRepeat, ResizeRequest,
    SEND_OUTPUT_REQUEST_FILE, SEND_OUTPUT_REQUEST_VERSION, SendOutputOptions, SendOutputRequest,
    WORKSPACE_REQUEST_VERSION, WorkspaceAction, WorkspaceDirection, WorkspaceOptions,
    WorkspaceRequest, is_borderless_fullscreen, parse_children_args, parse_directional_args,
    parse_directional_request, parse_inspect_args, parse_migrate_workspace_args,
    parse_migrate_workspace_request, parse_resize_args, parse_resize_request,
    parse_send_output_args, parse_send_output_request, parse_workspace_args,
    parse_workspace_request, render_directional_request, render_migrate_workspace_request,
    render_resize_request, render_send_output_request, render_workspace_request,
    resize_direction_valid, resize_mode_valid, resize_repeat_next, tick_summary_signature,
    verify_directional_argv_consistency, verify_migrate_workspace_argv_consistency,
    verify_resize_argv_consistency, verify_send_output_argv_consistency,
    verify_workspace_argv_consistency,
};

#[cfg(test)]
mod tests;
