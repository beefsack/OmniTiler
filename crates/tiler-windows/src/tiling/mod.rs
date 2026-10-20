#![forbid(unsafe_code)]
//! Tiling-only Windows runtime over the retained portable Engine.
//!
//! Portable policy lives here and compiles everywhere; native enumeration,
//! actuation, and the event loop live in [`crate::tiling_sys`] (`cfg(windows)`
//! only). The Engine is the layout authority: this crate builds complete
//! `Reconcile` observations, applies `CoreReply` geometry verbatim, and never
//! adopts observed rectangles into retained state.

use std::collections::HashMap;
use std::path::PathBuf;

use tiler_core::boundary::{CoreCommand, CoreEvent};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};

mod classification;
mod cli;
mod float;
mod geometry;
mod reconcile;
mod workspace;

pub use classification::{
    GENERIC_DIALOG_CLASS, OWN_SETTINGS_WINDOW_CLASS, ObservedTargetRef, SkipReason, TokenMap,
    WindowFacts, classify, fingerprint, float_toggle_refusal, is_own_settings_window,
    orientation_toggle_refusal, overlay_refusal, sticky_directional_refusal, sticky_toggle_refusal,
    unfloat_maximize_clear_eligible, workspace_dialog_refuses,
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
    FloatFocusCandidate, FullscreenToggle, RESTORE_WAKE_MS, StatelessVerdict,
    authorize_maximize_clear, born_keep_latch, born_maximized_keep_overlay,
    canonical_retained_rect, classify_focus, float_half_snap_rect, float_intent_marker_value,
    float_topmost_restore_needed, fullscreen_toggle_decision, half_snap_constraints_ok,
    inspect_stateless_verdict, maximize_clear_refusal, maximized_focus_fence,
    parse_float_intent_marker, parse_sticky_marker, parse_tile_override_marker,
    postclear_fullscreen_now, restore_wake_arm, restore_wake_step, retained_overlay_hint,
    select_float_focus_target, send_flags_stable, should_admit_slotless_maximized,
    should_clear_maximize_at_admission, should_clear_maximize_at_admission_with_mode,
    should_hold_born_fullscreen, should_seed_member_slot, stale_fullscreen_retire_eligible,
    sticky_marker_value, target_rect_valid, tile_override_marker_value, toggle_gate_outcome,
};
pub use geometry::{
    FrameInsets, INNER_GAP, OUTER_GAP, OUTPUT_ID, OWNER_ID, WORKSPACE_ID, hints_from_outer,
    min_hints_from_outer, normalize_max_track, normalize_min_track, normalize_outer_bound,
    overconstrained_effective, tiling_domain_bounds, tiling_domain_bounds_with,
    visible_max_from_outer, visible_min_from_outer,
};
pub use reconcile::{
    AllowEntry, GestureIntent, ObservedTarget, ReadbackOutcome, ReconcileInput, RefusedTracker,
    TRANSIENT_BASE_MS, TRANSIENT_MAX_MS, allow_match, allowlist_digest, build_reconcile_event,
    build_reconcile_event_for, build_reconcile_event_for_floating, classify_gesture,
    dpi_readback_decision, dpi_transition_reouter, drop_point_in_domain, parse_allowlist,
    prefer_modal_move, readback_outcome, transfer_readback_entry,
};
pub use workspace::{
    ChildrenOptions, DIRECTIONAL_REQUEST_FILE, DIRECTIONAL_REQUEST_VERSION, DirectionalOp,
    DirectionalOptions, DirectionalRequest, InspectOptions, RESIZE_REQUEST_FILE,
    RESIZE_REQUEST_VERSION, ResizeOptions, ResizeRepeat, ResizeRequest, SEND_OUTPUT_REQUEST_FILE,
    SEND_OUTPUT_REQUEST_VERSION, SendOutputOptions, SendOutputRequest, WORKSPACE_REQUEST_VERSION,
    WorkspaceAction, WorkspaceDirection, WorkspaceOptions, WorkspaceRequest,
    is_borderless_fullscreen, parse_children_args, parse_directional_args,
    parse_directional_request, parse_inspect_args, parse_resize_args, parse_resize_request,
    parse_send_output_args, parse_send_output_request, parse_workspace_args,
    parse_workspace_request, render_directional_request, render_resize_request,
    render_send_output_request, render_workspace_request, resize_direction_valid,
    resize_mode_valid, resize_repeat_next, tick_summary_signature,
    verify_directional_argv_consistency, verify_resize_argv_consistency,
    verify_send_output_argv_consistency, verify_workspace_argv_consistency,
};

#[cfg(test)]
mod tests;
