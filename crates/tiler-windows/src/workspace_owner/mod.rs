#![forbid(unsafe_code)]
//! Portable workspace-owner policy: output context, select/send resolution,
//! Engine observation assembly, and follow verification.
//!
//! Native effects (hide/reveal/focus/monitor identity) live in
//! `tiling_sys` (`cfg(windows)` only). This module owns no HWNDs, no window
//! properties, and no logs: pure resolution plus `CoreEvent` construction for
//! the retained `Engine` `SendToWorkspace`/`SendToOutput` routes.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::boundary::{CoreCommand, CoreEvent};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};

use crate::snapkey::SnapOp;
use crate::workspace::{ManagedWorkspaces, WindowKey, verify_send_follow};

mod directional;
mod foreground;
mod identity;
mod markers;
mod migration;
mod rows;
mod sends;

pub use self::directional::{
    DirectionalFenceLive, DirectionalFenceSnapshot, DirectionalOutput, DragDropFenceSnapshot,
    directional_fences_hold, directional_focus_crosses, directional_focus_target,
    directional_opposite, directional_pair_with_adjacency, drag_drop_fences_hold,
    drag_snapshot_for_directional, output_at_point, output_send_fences_hold,
    select_directional_target, verify_directional_transfer,
};
pub use self::foreground::{
    ForegroundFacts, ForegroundVeto, ForegroundVetoReason, classify_foreground,
};
pub use self::identity::{
    FixedAdmissionFlags, member_matches, output_context, reused_hwnd_stale,
    stamp_fixed_admission_flags, visible_lifetime_ok, workspace_domain,
};
pub use self::markers::{
    MarkerSlot, RestartMarker, classify_restart_marker, focus_before_geometry,
    restart_float_eligible, restart_sticky_eligible, restart_tile_override_eligible,
};
pub use self::migration::{
    MigrateFenceLive, MigrateFenceSnapshot, MigrateRow, build_migrate_event, migrate_fences_hold,
    migrate_snapshot_for_drag, select_migration_target, translate_rect_to_output,
    verify_migration_transfer,
};
pub use self::rows::{
    MemberView, PlannedWrite, dispatch_decision, domain_rows, hidden_snapshot_rect, planned_writes,
    rows_for, writable_subset,
};
pub use self::sends::{
    OwnerRow, SendFenceLive, SendFenceSnapshot, SendRoute, StayFocusClass, build_send_event,
    build_send_output_event, classify_stay_focus, directional_workspace_refusal,
    float_carry_tokens, send_fences_hold, send_overlay_gate, send_retained_overlay_admits,
    send_route, stamp_send_float_overlay, stamp_send_output_target, stamp_send_target,
    verify_membership_transfer,
};

#[cfg(test)]
mod tests;

/// R-SPC-04 D1-D8 + G-05 fixed-size admission regressions at the adapter
/// boundary: real retained `Engine` observations through the production
/// builders plus [`stamp_fixed_admission_flags`], the same stamping the
/// production handle path applies. No native calls, no HWNDs.
#[cfg(test)]
mod fixed_admission_tests;
