//! Native tiling-only runtime (`cfg(windows)` only).
//!
//! One product loop: PMv2 awareness verified first, full `EnumWindows`
//! enumeration per tick, retained [`tiler_core::engine::Engine`] as layout
//! authority, `SetWindowPos` actuation with independent readback, and
//! out-of-context `SetWinEventHook` callbacks delivered on the loop thread
//! itself (the loop pumps messages; no hook thread, no join). Stop leaves all
//! geometry untouched; there is no per-window geometry ledger and nothing is
//! ever hidden.
//!
//! Production log vocabulary is structured JSON with opaque window tokens
//! only: no HWNDs, pids, process creation strings, SIDs, paths, or titles.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, OwnedHandle};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply, NoGroupReason};
use tiler_core::directional::WindowId;
use tiler_core::engine::Engine;
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::size_hints::WindowSizeHints;
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, POINT, RECT, SetLastError};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    GetSystemMetricsForDpi, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
#[cfg(not(test))]
use windows_sys::Win32::UI::WindowsAndMessaging::GetParent;
use windows_sys::Win32::UI::WindowsAndMessaging::MsgWaitForMultipleObjectsEx;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, EnumChildWindows, EnumWindows, GW_OWNER,
    GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow,
    GetPropW, GetShellWindow, GetSystemMetrics, GetWindow, GetWindowLongW, GetWindowPlacement,
    GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, MINMAXINFO, MSG, PM_REMOVE, PeekMessageW, QS_ALLINPUT, RemovePropW,
    SM_CXMAXTRACK, SM_CXMINTRACK, SM_CXSCREEN, SM_CYMAXTRACK, SM_CYMINTRACK, SM_CYSCREEN,
    SMTO_ABORTIFHUNG, SW_MAXIMIZE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SetForegroundWindow,
    SetPropW, SetWindowLongW, SetWindowPlacement, SetWindowPos, ShowWindowAsync, TranslateMessage,
    WINDOWPLACEMENT, WINEVENT_OUTOFCONTEXT, WM_GETMINMAXINFO, WM_NCHITTEST,
    WPF_ASYNCWINDOWPLACEMENT, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_THICKFRAME,
};

use self::action_context::{ActionCtx, ActionLine, ActionTimings, SelectEffect, SendEffect};
#[cfg(test)]
use self::admission::admission_clear_planned;
use self::admission::{
    admission_clear_eligible, clear_maximize_at_admission, committed_ordinary_pin,
    engine_is_automatic_fixed, engine_is_float, fanout_pending_closed, fixed_origin_provenance,
    fixed_predicate_for, float_focus_candidates, float_send_admits, handle_engine_event,
    is_automatic_in, overlay_refusal_for, stamp_engine_event,
};
use self::assignments::{
    MAX_HIDE_DIAG_PER_SELECT, audit_hidden_claims, ensure_workspace_assignments,
    workspace_close_cleanup, workspace_hide_one, workspace_proof_reveal_gate,
};
use self::chord_resolution::{
    ChordObserved, ChordTarget, chord_overlay_refusal, observe_chord_intent,
    orientation_overlay_refusal, orientation_plan, resolve_chord_target, settle_chord_reject,
};
use self::cli_resize::poll_resize_cli_request;
use self::cli_workspace::poll_workspace_cli_request;
pub use self::commands::{
    cmd_capture, cmd_inventory, cmd_resize, cmd_tile, cmd_workspace, load_normal_tile_base,
};
#[cfg(test)]
use self::cross_drag::prepare_cross_drag_event;
use self::cross_drag::{
    CrossDragSettle, DropDestination, adopt_gaps_for_route, adopt_gaps_on_candidate,
    assemble_cross_drag_rows, cross_topology_fresh, fresh_cross_areas, resolve_drop_destination,
    settle_cross_drag_drop, snapshot_cross_drag,
};
use self::diag::{
    WindowDiagSample, diag_identity_broke, diag_identity_rebound, drop_diag_sample,
    flush_window_diag, stage_hint_samples, stage_no_plan, stage_write_samples,
};
#[cfg(test)]
use self::diag::{
    clear_diag_classes, clear_diag_pids, diag_class_cached, inject_diag_class, inject_diag_pid,
};
use self::directional_cross::run_directional_cross;
use self::directional_resolution::{
    chord_output, directional_arrival_verified, directional_live, directional_outputs_from_areas,
    directional_snapshot, drag_drop_live, output_for_point, output_for_rect,
    resolve_directional_target, revision_for, workspace_domain_for,
};
pub use self::dispatch::{LaunchBackend, SystemLaunchBackend};
use self::dispatch::{
    dispatch_float_intent, dispatch_launch_intent, dispatch_orientation_intent,
    dispatch_sticky_intent, dispatch_workspace_tiling_intent,
};
#[cfg(test)]
use self::drag_preview::prepare_cross_drag_preview;
use self::drag_preview::{prepare_cross_singleton_event, refresh_drag_preview};
use self::first_run::{FirstRunPending, resolve_first_run};
pub use self::first_run::{FirstRunResolution, resolve_first_run_outcome};
#[cfg(test)]
use self::float_toggle::FreshFloatObservation;
use self::float_toggle::{
    NativeUnfloatClear, UnfloatClearEffects, apply_float_from_tiled, apply_unfloat_to_tiled,
    clear_float_mark, clear_sticky_mark, restore_raised_band, unfloat_maximize_clear,
};
use self::focus::{hide_border, note_focus_advance, publish_managed, refresh_active_border};
pub(crate) use self::focus_actuation::owner_snap_dropped;
use self::focus_actuation::{
    FocusActuation, actuate_focus, born_maximized_keep_overlay_for, overlay_expected,
    same_axis_move_for,
};
#[cfg(test)]
use self::fullscreen::{
    FULLSCREEN_STYLE_BITS, OwnedExitEffects, decode_frame_value, encode_frame_value,
    exit_fullscreen_owned_with,
};
use self::fullscreen::{
    FullscreenMeta, clear_fullscreen_meta, effective_stale_meta_via_effects, enter_fullscreen,
    exit_fullscreen_owned, holdable_key, live_style_for_retire, read_fullscreen_meta,
    track_fullscreen_holds,
};
use self::geometry::{ApplyInput, ApplySummary, apply_geometry, audit_json, reply_outcome};
use self::gesture_tick::gesture_tick;
use self::group_underlay::{
    chord_held_now, classify_move_size_start, hide_preview, hide_underlay, live_outer_rect,
    refresh_group_underlay,
};
use self::hints::{
    HintCx, HintFallbackUse, HintOutcome, HintStats, LastGoodHint, hint_now, invalidate_last_good,
    note_last_good, timeout_candidate,
};
#[cfg(test)]
use self::hints::{clear_test_hints, inject_hint_now, inject_test_hints, inject_test_outcome};
use self::history::{
    log_workspace_action, move_float_to_output, move_overlay_to_output, remap_placement_to_output,
    workspace_history_tick, workspace_log,
};
use self::input::{
    InputBridge, log_input_faults, offer_input_control, offer_input_snapshot, poll_input_ack,
};
pub use self::inspection::{
    cmd_border_inspect, cmd_children, cmd_inspect, cmd_preview_inspect, cmd_underlay_inspect,
};
use self::inspection::{hosted_gate_allows, medium_caller};
use self::keyboard_resize::dispatch_resize_intent;
use self::keyboard_tick::keyboard_tick;
use self::migration::workspace_migrate_tick;
#[cfg(test)]
use self::migration::{migrate_rows_for, workspace_migrate_one};
#[cfg(test)]
use self::migration_run::migration_follow_tail;
use self::migration_run::run_workspace_migrate;
use self::migration_visibility::migration_view_effects;
#[cfg(test)]
use self::migration_visibility::{flush_migrate_hide_summary, flush_migrate_reveal_summary};
use self::minmax::{FreshHint, QueryCost, fixed_birth_for_slotless, hidden_hint_for, min_hint_for};
pub(crate) use self::monitors::ensure_pm_v2;
use self::monitors::{MonitorArea, all_monitors, monitor_fulls};
use self::observation::{cursor_pos, enumerate_hwnds};
#[cfg(test)]
use self::observe::WaitWake;
use self::observe::{
    HookEvent, ObservedWindow, OwnerWaitCounts, class_of, foreground_lifetime, hook_queue,
    observe_window, pump_wait, pump_wait_with_intent, verify_proof_owned, window_identity,
    winevent_proc,
};
pub use self::observe::{KnownIdentity, ObserveFailure};
use self::output_send::{resolve_output_send_target, run_output_send};
#[cfg(test)]
use self::output_sync::poll_foreground_workspace;
use self::output_sync::{
    foreground_elevated, sync_monitor_outputs, sync_monitor_outputs_local, workspace_maintenance,
};
use self::owner_work::OwnerWorkGuard;
use self::pending_drop::{PendingCrossDest, PendingDrop, queue_pending_drop, service_pending_drop};
#[cfg(test)]
use self::pending_seams::{
    APPLY_WRITABLES, INJECTED_APPLY, INJECTED_AREAS, INJECTED_ELEVATED, INJECTED_FLOAT_MOVE_OK,
    INJECTED_FLOAT_MOVES, INJECTED_FOCUS, INJECTED_ICONIC, INJECTED_NATIVE_MOVE_OK,
    INJECTED_NATIVE_TRANSFERS, INJECTED_OBSERVE, INJECTED_OVERLAY, INJECTED_STOP, INJECTED_SUSPEND,
    INJECTED_UNREADABLE, PENDING_COUNTERS, PendingCounters, clear_pending_seams,
    inject_monitor_areas, inject_pending_apply, inject_pending_apply_seq, inject_pending_arrival,
    inject_pending_arrival_placement, inject_pending_elevated, inject_pending_float_move_ok,
    inject_pending_focus, inject_pending_iconic, inject_pending_native_move_ok,
    inject_pending_observe, inject_pending_overlay_seq, inject_pending_placement_normal,
    inject_pending_recovery, inject_pending_stop, inject_pending_suspend, inject_pending_tag,
    inject_pending_unreadable, pending_apply_writables, pending_counters, pending_native_transfers,
};
use self::pending_seams::{
    fresh_arrival_rect, live_is_iconic, live_member_tag, live_query_recovery,
};
#[cfg(not(test))]
use self::placement::placement_is_workspace_coords;
use self::placement::{
    fresh_arrival_placement, live_placement_workspace_coords, screen_to_placement_raw,
    translate_placement_normal_to_target,
};
#[cfg(test)]
use self::placement::{live_placement_normal, placement_raw_to_screen};
use self::preview::{
    PreviewBound, PreviewFresh, PreviewGate, PreviewStartBinding, preview_hide_for_refusal,
    preview_prior_desc, preview_same_size, preview_sample_gate,
};
pub use self::probes::live_overlay_flags;
use self::probes::{
    WriteTarget, is_zoomed_now, pid_current, restore_wake_probe, restore_zoom_placement,
    revalidate_target, toggle_zoom_async,
};
pub use self::proof_commands::{
    cmd_hide_proof, cmd_shortcut_proof, cmd_tile_proof, cmd_workspace_proof,
};
#[cfg(test)]
use self::reconcile::sweep_retire_stale_fullscreen;
use self::reconcile::{reconcile_tick, writable_tokens};
#[cfg(test)]
use self::render_ack::{RENDER_FRAME_SCOPE, RENDER_SUMMARY_WINDOW, RenderFrameSample};
use self::render_ack::{
    RenderFrameHealth, note_render_hidden, render_end_snapshot, render_same_thread_gate,
    same_thread_sample,
};
use self::render_bridge::{
    RenderBridge, drain_render_failures, hide_preview_finish, hide_render_carrier,
    owned_overlay_hwnd, poll_render_acks, post_border_frame, post_preview_frame,
    post_underlay_frame, render_carrier_visible,
};
use self::restart::{admit_born_fullscreen, admit_slotless_maximized, adopt_restart_markers};
use self::retained::{HiddenRecord, PendingClosed, RetainedRow};
use self::rows::{
    assemble_domain_rows, drop_member_state, hold_band_target, hold_marker_target,
    hydrate_tile_override_marker, install_float_mark_held, install_sticky_mark_held,
    install_tile_override_mark_held, marker_tag_pid_fresh, prune_float_state,
    remove_tile_override_mark_best_effort, retain_float_focus_validated,
};
use self::run_loop::run_tile_loop;
use self::run_loop_intake::{Intake, pump_intake};
use self::run_loop_tick::{TickFlow, run_active_tick};
use self::run_loop_wait::drain_hook_edges;
use self::selection::remember_select_departure;
use self::selection::workspace_do_select;
use self::send_follow::workspace_send_follow;
use self::send_native::workspace_do_send_native;
use self::send_preflight::SendPreflight;
use self::send_preflight::SendTarget;
use self::send_stay::ensure_trailing_spare;
use self::send_stay::workspace_send_stay;
use self::sends::workspace_do_send;
#[cfg(test)]
use self::settings::apply_live_settings;
use self::settings::{TileRun, poll_live_settings, suspend_read};
use self::shell::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, exe_file_name, is_border_shell_popup,
    is_shell_class, rect_from_win,
};
use self::trace::{log_engine_placement_trace, log_json_at, log_mono_ms, refresh_border_timed};
#[cfg(test)]
use self::visibility_diag::{
    REHIDE_DIAG_MAX_ENTRIES, REVEAL_DIAG_PER_SELECT, clear_visibility, inject_visibility,
};
use self::visibility_diag::{
    VisibilityDiagnostics, drop_visibility_sample, flush_exceptions, flush_rehide_diag,
    flush_visibility, gesture_float_cause, note_hide_diag, note_rehide_diag, note_reveal_diag,
    omission_summary, rehide_err_outcome, rehide_retired_detail, reveal_err_outcome,
    reveal_log_line, reveal_outcome, rollback_newly_hidden, stage_exceptions, stage_visibility,
};
#[cfg(test)]
use self::windrag::clear_windrag_gesture_scoped;
use self::windrag::{
    capture_gesture_start, clear_windrag_gesture, esc_edge_cancels, windrag_down, windrag_up,
};
#[cfg(test)]
use self::workspace_mode::dispatch_workspace_release;
use self::workspace_mode::read_topmost_now;
#[cfg(test)]
use self::workspace_mode::reset_fixed_admission_for_domain;
use self::workspace_mode::retry_pending_releases;
use self::workspace_mode::set_topmost_band;
use self::workspace_mode::toggle_workspace_tiling;
use self::workspace_mode::workspace_mode_known;
use self::workspace_mode::workspace_mode_tiled;
use self::workspace_navigation::workspace_tick;
use self::workspace_send_tick::{workspace_output_send_tick, workspace_relative_send_tick};
use crate::active_border::{
    ActiveBorderOptions, border_eligible, border_outer_rect, scale_style, scale_to_physical,
};
use crate::active_border_sys::{
    BorderOverlay, OverlayOutcome, PreviewOverlay, UnderlayOverlay, lowest_in_z,
};
use crate::group_underlay::{
    GroupUnderlayOptions, MoveSizeKind, aggregate_chord_keys, classify_hit_test, underlay_eligible,
    underlay_geometry_debug, underlay_outer_rect,
};
use crate::input_handoff::InboxItem;
use crate::input_handoff::SharedSnapshot;
use crate::input_handoff::fence_base_generation;
use crate::input_handoff::note_policy_published;
use crate::input_handoff::snapshot_content_eq;
use crate::input_handoff::snapshot_policy_eq;
use crate::input_health::{Health, InputSample, OwnerSample};
use crate::input_pump::READY_WAIT;
use crate::input_pump::{FailEvent, InboxSender, InputHandle};
use crate::lifecycle::{
    WORKSPACE_REQUEST_FILE, exe_paths_equal, is_medium_rid,
    sys::{log_path_for, snap_resume, snap_suspend, stop_requested},
};
use crate::model::ProcessIdentity;
use crate::native::HeldProcess;
use crate::render_handoff::{
    BorderDesired, CarrierAck, DesiredFrame, PreviewDesired, RenderCarrier, RenderChannels,
    RenderOutcome, UnderlayCandidate, UnderlayDesired, WindowLifetime,
};
use crate::render_pump::native::{RenderFailEvent, RenderHandle};
use crate::render_pump::{FINISH_WAIT, RENDER_READY_WAIT, RENDER_STOP_WAIT};
use crate::settings::LiveSettings;
use crate::snapkey::{
    KeyboardConfig, LaunchKind, MAX_DISPATCH_PER_TICK, OriginVerdict, QueuedFloatIntent,
    QueuedLaunchIntent, QueuedSnapEvent, QueuedStickyIntent, QueuedWorkspaceHistoryIntent,
    QueuedWorkspaceMigrateIntent, QueuedWorkspaceOutputIntent, QueuedWorkspaceSendIntent, SnapEdge,
    SnapOp, SnapOrigin, VK_LSHIFT, VK_LWIN, VK_MASK, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    WorkspaceHistoryOp, WorkspaceOp, direction_name, resolve_origin,
};
use crate::storage::LedgerStore;
use crate::tiling::{
    AllowEntry, CaptureOptions, ChildrenOptions, FrameInsets, GestureIntent, HideProofOptions,
    INNER_GAP, InspectOptions, OUTER_GAP, OWNER_ID, ObservedTarget, ObservedTargetRef,
    RESIZE_REQUEST_FILE, ReadbackOutcome, RefusedTracker, ResizeOptions, ScopeHostChild,
    SkipReason, StatelessVerdict, TileOptions, TileProofOptions, TokenMap, WindowFacts,
    WorkspaceAction, WorkspaceOptions, allow_match, allowlist_digest, classify, classify_gesture,
    drop_point_in_domain, fingerprint, fullscreen_toggle_decision, hosted_child_allows,
    inspect_stateless_verdict, is_borderless_fullscreen, parse_allowlist, parse_resize_request,
    parse_workspace_request, readback_outcome, scope_allows, scope_exe_basename,
    should_hold_born_fullscreen, tick_summary_signature, tiling_domain_bounds_with,
};
use crate::win_mouse::sys::WinDragPublished;
use crate::win_mouse::{
    WINDRAG_MAX_DISPATCH_PER_TICK, WinDragEdge, WinDragKind, WinDragSnapshot, WinSettle,
    settle_pointer_journey, validate_windrag_down,
};
use crate::workspace::ManagedWorkspaces;
type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

const TICK_POLL_MS: u32 = 100;
const SLOW_POLL_MS: u64 = 2000;
// Pumped focus-settle bound: re-reads of `GetForegroundWindow` after an
// accepted setter, 50 ms pumped each so the LL-hook message pump stays live.
// No foreign wait: `pump_wait` only services the owner's own queue.
const FOCUS_SETTLE_ROUNDS: u32 = 10;
const FOCUS_SETTLE_POLL_MS: u32 = 50;

/// Unconfirmed workspace-mode toggle releases queue in
/// [`crate::workspace::PendingReleases`]: keyed by domain with
/// latest-intent-wins so a stale float can never fire at a retiled layout,
/// dropped on confirmed toggles, stale once tiled, and retried only on
/// topology edges with current observed geometry (never polled).
struct TileLoop {
    engine: Engine,
    owner: OwnerId,
    generation: GenerationId,
    tokens: TokenMap,
    refused: RefusedTracker,
    /// Stable pre-gesture rectangles from the last reconcile observation.
    stable: HashMap<u64, Rect>,
    /// Managed HWNDs from the last reconcile observation.
    managed: HashSet<u64>,
    /// Open user gestures on managed windows only.
    active: HashSet<u64>,
    /// Pre-gesture rectangles captured at START (from `stable`, never from a
    /// mid-gesture frame; live `GetWindowRect` fallback only when `stable`
    /// has no entry, e.g. a window admitted between ticks).
    gesture_before: HashMap<u64, Rect>,
    /// Release cursor captured at MOVESIZEEND event-callback time, carried
    /// to settle. Settle never re-reads the cursor later: an END without a
    /// captured cursor routes as no-change for moves. Cleared on
    /// END-settle/removal like `gesture_before`.
    gesture_end_cursor: HashMap<u64, (i32, i32)>,
    /// START-time identity snapshot per open gesture (pid/creation from a
    /// bounded owner-side probe plus the live visible-membership lifetime
    /// tag). Settle requires the fresh window to still match this snapshot in
    /// addition to the stored member, so a reuse between START and END fails
    /// closed even when HWND/PID/creation still agree. Absent entries fall
    /// back to the stored-member check only. The tag is the member property
    /// (`OmniTilerMember`), never the owned-helper property (which is
    /// empty for ordinary windows and would refuse every ordinary gesture).
    gesture_start_key: HashMap<u64, crate::workspace::WindowKey>,
    gesture_start_tag: HashMap<u64, Option<String>>,
    tick: u64,
    allowlist: Option<Vec<AllowEntry>>,
    /// Explicit normal-mode scope filter (top-level exe basenames,
    /// normalized at parse). Empty means no filter: every eligible window is
    /// managed. Nonempty fences observation, geometry writes, workspace
    /// hide/reveal membership, and focus actuation.
    scope: Vec<String>,
    /// Explicit host-to-child scope pairs (empty means no child constraint).
    /// A listed host passes observation and hide admission only while a live
    /// hosted child matches, so a newly appearing hosted app can never ride
    /// another app's host executable into management.
    scope_hosts: Vec<ScopeHostChild>,
    trace: bool,
    suspended: bool,
    last_summary: Option<String>,
    log_path: std::path::PathBuf,
    audit_path: Option<std::path::PathBuf>,
    /// Product keyboard policy for this run (takeover switch + Win+L opt-in).
    /// Live settings edits update this in place; the hook gate republishes
    /// every pump and the classifier applies policy plus the rebound table
    /// through `apply_live_config`.
    keyboard: KeyboardConfig,
    /// Live inner/outer gaps (KDE 0..64). Saved settings supply the startup
    /// base and explicit CLI switches override; live file edits flip these
    /// and adopt through the retained update-gaps path, never a reseed.
    inner_gap: i32,
    outer_gap: i32,
    /// Live settings polling for the normal owner only (`None` on proof
    /// paths, which keep deterministic explicit configuration and never read
    /// the user's store).
    settings_live: Option<LiveSettings>,
    /// Settings directory for live polling (mirrors `settings_live`).
    settings_dir: Option<std::path::PathBuf>,
    /// Last applied rebound-chord table, so binding-only edits re-apply even
    /// when the keyboard policy is unchanged.
    last_remap: Vec<crate::snapkey::ChordRemap>,
    /// Last applied suppression table (disabled or rebound-away chords), same
    /// re-apply contract as the rebound table.
    last_disabled: Vec<crate::snapkey::ChordDisable>,
    /// Explicit startup CLI switches, authoritative for the whole run: every
    /// live settings-file apply re-applies these lanes over the file base.
    cli_overrides: crate::tiling::CliOverrides,
    /// Last published snap-queue loss count, for explicit drop evidence.
    snap_dropped: u32,
    /// Last published callback-summary loss counts, for explicit drop/filter
    /// evidence on the trace-only `snap-callback` drain.
    cb_diag_dropped: u32,
    cb_diag_filtered: u32,
    /// Last published callback-summary outbox drops (product pump only).
    cb_outbox_dropped: u32,
    /// Last published trace-only mask-send aggregate (`sends`, `max_us`).
    mask_sends: u32,
    mask_send_max_us: u32,
    /// Chord-time origin snapshots for the keyboard callback, refreshed with
    /// every complete observation alongside `managed`.
    snap_origins: HashMap<u64, SnapOrigin>,
    /// Tiled-only managed origins for the Win+Left mouse callback,
    /// refreshed with every complete observation alongside `managed`.
    /// Floating and sticky members are excluded so their Win+Left passes
    /// through natively (titlebar-only); maximized/fullscreen members are
    /// retained, never observed, so they never appear here either.
    /// Each entry also carries the member lifetime tag read at publish
    /// time, so the hook can snapshot it into the Down edge with no native
    /// reads in the callback. Exactness stays with the per-edge owner
    /// recheck.
    windrag_origins: HashMap<u64, WinDragPublished>,
    /// Settle producer per open gesture HWND: `"native"` for the
    /// title-bar modal loop, `"windrag"` for the project-driven Win+Left
    /// stationary hold. Absent entries settle as native. Cleared on
    /// END-settle/removal like `gesture_before`.
    gesture_producer: HashMap<u64, &'static str>,
    /// Down-time pointer per open windrag gesture HWND, bound in the hook
    /// callback at down time. Settle compares it against the callback-bound
    /// release pointer (pointer delta, not rect delta: the real window
    /// keeps its source allocation mid-gesture). Cleared on END-settle/
    /// removal like `gesture_before`.
    windrag_start_cursor: HashMap<u64, (i32, i32)>,
    /// Validated Down-time bound snapshot per open windrag gesture HWND.
    /// Stored at arm time after the snapshot matches fresh evidence; the
    /// Up requires this entry (bound matching, never a rebind), and settle
    /// consumes it alongside the START fence. Cleared on END-settle/
    /// removal like `gesture_before`.
    windrag_bound: HashMap<u64, WinDragSnapshot>,
    /// Last published windrag-queue loss count, for explicit drop evidence.
    windrag_dropped: u32,
    /// Last logged tiled-origin count for the mouse hook, for change-only
    /// `windrag-origins` evidence (proves the hook binds against a live
    /// managed set without logging identity).
    windrag_origin_logged: Option<usize>,
    /// Last logged hook delivery counters, for change-only
    /// `windrag-hook-stats` evidence splitting callback delivery from owner
    /// validation.
    windrag_stats_logged: (u64, u64, u64, u64),
    /// Verified own-focus continuation across bounded drains. Set only on an
    /// exact verified owner actuation (`focus-ok`); cleared on external
    /// focus, lifetime mismatch, suspension, or gesture. Lets a stale chord
    /// origin continue from our own advance within and across batches, never
    /// a permissive retarget.
    snap_advance: Option<SnapOrigin>,
    /// Exact KDE `requestResize` repeat state for keyboard resize: the
    /// focused window, direction, and mode of the last dispatched resize
    /// plus the next press index. Keyed by [`crate::tiling::ResizeRepeat`];
    /// continued only on an exact three-way match, restarted otherwise.
    /// Key-up never resets (KDE parity). Updated only when a resize passes
    /// every pre-dispatch fence, before the Engine dispatch.
    resize_repeat: Option<crate::tiling::ResizeRepeat>,
    /// Raw `EnumWindows` count from the latest observation (targets seen
    /// before any eligibility filtering, including zero).
    last_enumerated: usize,
    /// Project-owned per-output-local workspaces. Independent Engine domains
    /// per `(output, workspace)` preserve each layout plus last focus.
    workspaces: crate::workspace::ManagedWorkspaces,
    /// Startup-adopted workspace-scope mode plus scoped history (item 12
    /// stage B). Adopted once from the saved mode before `sync_monitor_outputs`;
    /// live file edits never adopt, only report restart-required drift.
    scope_owner: crate::workspace_scope_owner::WorkspaceScopeOwner,
    /// Stable session tokens per member key. The token (not the ephemeral
    /// product nonce) is the Engine identity; the nonce lives only in
    /// `hidden_claims` plus the ledger while hidden.
    member_tokens: std::collections::BTreeMap<crate::workspace::WindowKey, String>,
    /// Last-known rectangles per member token (visible reads plus hidden
    /// snapshots) so Engine source/target observations stay complete.
    member_rects: HashMap<String, Rect>,
    /// Known hidden product claims by stable member key. Retained
    /// independently of the `IsWindowVisible` observation gate so hidden
    /// windows are never dropped; the nonce leaves only after verified
    /// reveal or release. Keyed by the full `(hwnd, pid, creation)` member
    /// identity, never HWND alone, so a recycled HWND never matches.
    hidden_claims: std::collections::BTreeMap<crate::workspace::WindowKey, HiddenRecord>,
    /// Stored full process identity per member key. Hide effects bind to
    /// this stored identity and refuse recycled HWNDs with no writes.
    member_identity: std::collections::BTreeMap<crate::workspace::WindowKey, ProcessIdentity>,
    /// Stored visible-membership lifetime tag per member key, stamped at
    /// admission onto the window itself. Same-process HWND reuse starts
    /// without it, so every effect re-verifies live-against-stored before
    /// trusting HWND/PID/creation. Never serialized, never trusted across
    /// runs, never removed (the property dies with its window).
    member_tags: std::collections::BTreeMap<crate::workspace::WindowKey, String>,
    /// Cached output context for the next select. Updated on every switch
    /// and refreshed from pointer/foreground context when stale.
    active_output: String,
    /// Last genuine focused output (foreground-monitor derived, never the
    /// owned tray/menu window): preserved across the transient menu
    /// foreground so the tray click point never becomes a focus change.
    /// Updated only from a known real foreground monitor; `None` until the
    /// first one resolves.
    last_genuine_output: Option<String>,
    /// Taskbar pager opt-out (settings `core.taskbar_pager`, default on).
    /// Live edits hot-apply through the settings lane; proof owners never
    /// page regardless.
    taskbar_pager: bool,
    /// Live overlay owner (normal owner only). Created lazily on the first
    /// safe pump; RAII teardown covers every exit path.
    pager_overlays: Option<crate::pager_sys::PagerOverlays>,
    /// Loop-owned latest pager click slot: overlay windows overwrite it, the
    /// tick dispatch drains it once. Owned here (not by the overlays) so
    /// suppression drops a staged click even with no windows present, and
    /// tests can stage clicks with no native windows at all. Only the newest
    /// click survives, so a stale cell never replays after a newer one.
    pager_clicks: Arc<Mutex<Option<crate::tray_pager::SelectScope>>>,
    /// Outputs with a currently served strip (last safe present): clicks
    /// naming any other output refuse as not-served, never dispatched.
    pager_served: Vec<String>,
    /// Owner-registration retry gate for the pager overlays: fixed 5s delay,
    /// forever, so a failed registration retries promptly without
    /// busy-looping pumps.
    pager_retry_at: Option<Instant>,
    /// Slow secondary-taskbar enumeration deadline plus the last topology
    /// fingerprint. The primary bar is re-read every pump; secondaries only
    /// here (they only feed refusal logging, never positions).
    pager_revalidate_at: Instant,
    pager_areas_fp: u64,
    /// Cached primary taskbar facts from the last pump (fresh, <=100ms) and
    /// cached secondaries from the last slow revalidate.
    pager_primary: Option<crate::pager::TaskbarFacts>,
    pager_secondaries: Vec<crate::pager::TaskbarFacts>,
    /// Live identity-verified pager frames per managed member key, refreshed
    /// on every observation alongside `overlay_now` (visible members plus
    /// retained fresh frames; hidden members fall back to their saved
    /// tile-slot/float snapshots in the portable resolver). Rebuilt per pass,
    /// so stale entries never survive their observation.
    pager_live: HashMap<crate::workspace::WindowKey, Rect>,
    /// Identity-keyed pager icon cache (worker-owned artwork bytes). Pruned
    /// to live membership every pump; never queried or waited on in the
    /// tile loop or the painter.
    pager_icon_cache: crate::pager_icons::PagerIconCache,
    /// Single background icon lookup worker plus its bounded queues. Created
    /// lazily on the first safe pump; detached teardown (never joined).
    pager_icon_service: Option<crate::pager_icons_sys::PagerIconService>,
    /// Icon worker spawn retry gate (fixed 5s delay, forever): a failed
    /// spawn retries promptly without busy-looping pumps while the pager
    /// stays icon-less meanwhile.
    pager_icon_retry_at: Option<Instant>,
    /// Last pager suppression outcome (`None` while showing): change-only
    /// `pager-suppress` logging.
    pager_suppress_last: Option<String>,
    /// Last per-output pager refusal reason by device key: change-only
    /// `pager-refused` logging, never guessed positions.
    pager_refusals: HashMap<String, String>,
    /// True only for the `workspace-proof` command: workspace hides are
    /// allowed, but solely for frozen-allowlist helpers verified fresh via
    /// `verify_proof_owned` before every write. Normal `tile` allows
    /// ordinary-app hides; `shortcut-proof` and `tile-proof` never hide.
    workspace_proof: bool,
    /// One-shot maximize-clear attempts at admission (KDE
    /// `maximizeAdmissionAttempts` parity): the first non-fullscreen
    /// admission of a maximized window without a retained tiled slot restores
    /// the native maximize exactly once with no automatic retry. Keyed by
    /// HWND plus process creation so a recycled HWND re-arms for the fresh
    /// window while the same window never retries.
    maximize_admission_attempted: HashSet<String>,
    /// R-MAX-06 keep-overlay latch: exact fresh lifetimes bound at tiled-first
    /// slotless admission under the live overlay value. Keyed by full member
    /// identity, never HWND alone; pruned with membership. Pre-existing and
    /// born-fullscreen lifetimes never enter; floating-first (R-MAX-03) binds
    /// `retile_keep_overlay` below instead, so the born tile default keeps
    /// clearing tiled-first admissions while retile always overlays.
    born_keep_overlay: BTreeSet<crate::workspace::WindowKey>,
    /// R-MAX-03 retile latch: exact fresh lifetimes bound at floating-first
    /// slotless admission. A floating-to-tiled workspace admission keeps the
    /// native maximize over the reserved tile slot with no one-shot clear,
    /// independent of `core.born_maximized` (only born R-MAX-06 reads that
    /// setting). Keyed by full member identity, never HWND alone; pruned
    /// with membership like `born_keep_overlay`.
    retile_keep_overlay: BTreeSet<crate::workspace::WindowKey>,
    /// Born-fullscreen hold (KDE initial-fullscreen-hold parity): first-seen
    /// fullscreen windows without a retained tile slot, tracked slotless as a
    /// planner-only exception until their first exit. Keyed by full member
    /// identity, never HWND alone. A window already seen non-fullscreen in
    /// this lifetime never enters.
    born_fullscreen: BTreeSet<crate::workspace::WindowKey>,
    /// Lifetime-known non-fullscreen member identities: every admitted or
    /// observed-eligible member lands here, so a later fullscreen transition
    /// is a managed overlay, never a born hold again. Pruned with the
    /// enumeration like the hold set.
    seen_nonfullscreen: BTreeSet<crate::workspace::WindowKey>,
    /// Last observed foreground HWND. Only an actual foreground change to a
    /// hidden member selects its workspace; event-only notifications never do.
    last_foreground: u64,
    /// Monitor device keys from the last loop pass, for disconnect/reconnect.
    known_outputs: Vec<String>,
    /// Raw `EnumWindows` HWND inventory from the latest enumeration,
    /// independent of eligibility: close cleanup and retained-occupancy
    /// decisions never mistake an unclassified window for a closed one.
    last_hwnds: HashSet<u64>,
    /// D23 minimized-omission set: member keys observed iconic this tick.
    /// Their Engine rows are omitted so convergence releases the leaf,
    /// reflows survivors, resolves MRU-or-none focus, and stores the
    /// `MinimizedSlot`. Populated in `observe`, consumed in
    /// `assemble_domain_rows`; cleared on every real observation pass.
    /// Session-only, never persisted.
    minimized_omitted: HashSet<crate::workspace::WindowKey>,
    /// D23 verified-close queue: Engine tokens with live-time homing,
    /// carried as `closed_windows` on that domain's next valid observation.
    /// Cleared only on Engine acceptance; every rejection retains. Stale
    /// entries (the id live again) drop at stamp time, never dispatching a
    /// contradictory live+closed row. Session-only, never persisted.
    pending_closed: std::collections::BTreeMap<String, PendingClosed>,
    /// Monitor snapshot from the last loop pass: disconnect-time geometry
    /// for the survivor chooser, never post-disconnect frames as proxy.
    last_areas: Vec<MonitorArea>,
    /// Last logged minimum-size hint per Engine token, for bounded
    /// hint-change summaries plus the lifetime-bound retained overlay reuse:
    /// a retained tiled maximized/fullscreen member rides its last-known
    /// declared hint until a normal fresh query resumes. Keyed by Engine
    /// token (never HWND alone) so a recycled HWND mints a fresh token and
    /// never inherits; pruned by live membership plus row presence, and
    /// dropped immediately with member state. Float, born-hold, hidden-fresh,
    /// minimized, and cloaked rows never reuse it.
    hint_logged: HashMap<String, WindowSizeHints>,
    /// Last successful declared-hint state per live member key: the
    /// behavioral fallback for a later native query Timeout on that same
    /// live member, with no time bound. Separate behavioral state from the
    /// `hint_logged` diagnostics cache above (which row assembly overwrites
    /// per row): fresh usable queries store (replacing immediately),
    /// successful-but-empty queries clear, and unperformed, failed, Timeout,
    /// and budget-skipped queries leave it untouched. Keyed by full member
    /// identity plus the opaque token/tag lifecycle; dropped only with member
    /// state, reveal retirement, or a moved native identity. Domain release
    /// and float toggles never drop it.
    hint_last_good: HashMap<crate::workspace::WindowKey, LastGoodHint>,
    /// Lightweight per-token Timeout-reuse report (`reused`, entry age, tick)
    /// for minimum-hint diagnostics. Written on every fresh-query lane row;
    /// pruned with membership. No log lines here.
    hint_fallback_uses: HashMap<String, HintFallbackUse>,
    /// Latest per-window diagnostic sample per member token (hint lane from
    /// row assembly, write lane from geometry application). Completed samples
    /// stay flush-eligible across ticks until they emit. Pruned with
    /// membership like the hint caches above.
    diag_pending: HashMap<String, WindowDiagSample>,
    /// Last emitted per-window sample per token: the stable-change dedupe
    /// baseline (advisory timing and labels excluded). Dropped on identity
    /// breaks (stale window, not comparable). Pruned with membership.
    diag_emitted: HashMap<String, WindowDiagSample>,
    /// Emission-time window class per token, resolved once per member
    /// lifetime and only for entries that actually emit. An identity break
    /// marks it unknown (empty) until a trustworthy query re-binds. Pruned
    /// with membership.
    diag_class: HashMap<String, String>,
    /// Last wall time a `window-diag` event emitted (test-pinned hint clock
    /// in unit tests, live time in production). `None` until the first emit.
    diag_last_emit: Option<Instant>,
    /// Fair emission order: per-token sequence of last emission (0/absent
    /// means never emitted). Never-emitted and oldest-emitted tokens emit
    /// first so a flapping head cannot starve the tail. Pruned with
    /// membership.
    diag_emit_seq: HashMap<String, u64>,
    /// Monotonic clock for [`TileLoop::diag_emit_seq`].
    diag_emit_clock: u64,
    /// Tokens merged by throttled rounds since the last emission (unique:
    /// a token counts once no matter how many rounds it waits through).
    /// Reported as `coalesced`, cleared on emit, pruned with membership.
    diag_coalesced: HashSet<String>,
    /// Latest row-assembly hint-query elapsed milliseconds: advisory timing
    /// for the whole assembly across every domain in the operation, carried
    /// on the next `window-diag` event. Per-window native cost rides each
    /// entry (`timeout_ms`, `query_us`) instead.
    diag_query_ms: u64,
    /// Bounded normal-level visibility/exception diagnostics (latest samples,
    /// emitted baselines, throttle clocks, fair order, coalesced tokens, and
    /// lost-intermediate flags). Pruned by live membership like the hint
    /// caches above.
    vis_diag: VisibilityDiagnostics,
    /// Monotonic start of the current fullscreen suspension, for the resume
    /// elapsed timing. Set on suspend, consumed on resume; logging only.
    suspend_started: Option<Instant>,
    /// Prompt-restore wake armed by one async restore dispatch, cleared only
    /// after a gated reconcile consumes it, on expiry, or on member loss.
    restore_wake: Option<RestoreWake>,
    /// Active-border configuration for this run (default on with an explicit
    /// off flag). The border never takes geometry writes: it only reads the
    /// foreground target's fresh frame and paints the process-owned overlay.
    border: ActiveBorderOptions,
    /// Process-owned border overlay plus the last logged border signature
    /// (visibility/rect/style/target) for change-only logging.
    border_overlay: BorderOverlay,
    border_last: Option<String>,
    /// Group-underlay configuration for this run (default on with an explicit
    /// off flag). Like the border it never takes geometry writes: it resolves
    /// the focused window's immediate-parent group through the retained
    /// Engine and paints the process-owned fill beneath the lowest member.
    underlay: GroupUnderlayOptions,
    /// Process-owned underlay fill plus the last logged underlay signature
    /// for change-only logging.
    underlay_overlay: UnderlayOverlay,
    underlay_last: Option<String>,
    /// Classified open move/size gestures by HWND, sampled once at START via
    /// the official `WM_NCHITTEST` result. Only `Move` on the actual focused
    /// window feeds the underlay trigger; resize (or unknown) never does.
    /// Maintained alongside `active`, cleared on END/removal like
    /// `gesture_before`.
    move_kind: HashMap<u64, MoveSizeKind>,
    /// START-time pointer per open native gesture HWND, sampled at
    /// MoveSizeStart drain time. The preview zero gate compares the live
    /// cursor against this point (no preview on a title click with no
    /// movement); the windrag path uses `windrag_start_cursor` instead.
    /// Cleared on END-settle/removal like `gesture_before`.
    gesture_start_cursor: HashMap<u64, (i32, i32)>,
    /// Process-owned drop-preview fill plus the last logged preview
    /// signature for change-only logging. Separate carrier above windows
    /// (KWin overlay-item analogue); never mixed with the border/underlay
    /// below-target plane. Shows only a resolved Engine target slot while
    /// an accepted move gesture holds the loop; never focuses, never
    /// writes geometry, never changes topology.
    preview_overlay: PreviewOverlay,
    preview_last: Option<String>,
    /// Per-sample carried sticky group-edge hover prior per open gesture
    /// HWND. Feeds every preview sample and the final drop through the same
    /// Engine resolver. Identity/output/workspace/revision binding lives in
    /// `gesture_preview_start` (frozen at START, never rebound); drift fails
    /// the gesture closed via `preview_dead`. Cleared on
    /// finish/cancel/suspension/invalidation like `gesture_before`.
    preview_bound: HashMap<u64, PreviewBound>,
    /// START-frozen preview source binding per open gesture HWND: exact
    /// mover token, source output/workspace, and accepted revision. The
    /// preview never rebinds mid-gesture; drift fails the gesture closed
    /// (see `preview_dead`) until settle. Cleared on END-settle/removal
    /// like `gesture_before`.
    gesture_preview_start: HashMap<u64, PreviewStartBinding>,
    /// Fail-closed preview gestures: drift or invalidation killed the
    /// preview for this hold, and no later sample may rebind or render
    /// until settle clears. Cleared on END-settle/removal like
    /// `gesture_before`.
    preview_dead: HashSet<u64>,
    /// Escape latched per open managed gesture HWND from the explicit
    /// keyboard-hook down edge while the gesture holds the loop. An
    /// Esc-cancelled native move restores natively to its pre-gesture frame,
    /// so rect comparison alone cannot tell cancel from zero-move: the latch
    /// makes cancellation explicit at settle time. Cleared on END/removal
    /// like `gesture_before`. Hookless runs never set it.
    esc_latched: HashSet<u64>,
    /// Esc sequence snapshot per open gesture HWND, taken at START from the
    /// keyboard-hook monotonic counter. Only edges with a newer sequence
    /// latch (pre-gesture taps never cancel; fast taps arriving with the END
    /// batch still latch via the single post-loop drain). Cleared on
    /// END/removal like `gesture_before`.
    gesture_esc_seq: HashMap<u64, u64>,
    /// Esc sequence snapshot per open gesture HWND, taken at END from the
    /// keyboard-hook monotonic counter (event-callback time, alongside the
    /// Finish pointer). Ended gestures latch only when this differs from
    /// START, so a physical Esc after the END but before the owner drain can
    /// never cancel a completed drop. Absent entries never latch (fail
    /// closed to no-cancel; settle then classifies from geometry). Cleared
    /// on END/removal like `gesture_before`.
    gesture_end_seq: HashMap<u64, u64>,
    /// Last observed Win+Shift level for the chord-edge wake. Sampled every
    /// pump before the idle skip so a bare chord press/release wakes the
    /// visual refresh on the 100ms cadence instead of waiting for the 2s
    /// slow poll. Steady hold/release stays quiet; only the transition
    /// wakes, and the normal suspend/fullscreen path below still gates it.
    underlay_chord_last: bool,
    /// Prior `WS_EX_TOPMOST` per intentional-float member key, read at float
    /// time. Session-local only. Unfloat and graceful stop restore only bands
    /// the project raised, with no frame change.
    float_topmost_prev: std::collections::BTreeMap<crate::workspace::WindowKey, bool>,
    /// Live float frame per Engine token. Distinct from `member_rects`, which
    /// keeps the last tiled allocation frozen at float time.
    float_rects: HashMap<String, Rect>,
    /// Intentional per-window float lifetimes (exact `(hwnd, pid, creation)`
    /// identity, never HWND alone). The Engine exception is dropped by every
    /// domain release, so this carries the exception across floating
    /// release/retiling and boundary sends: row assembly rides these tokens
    /// floating even when the session carries no exception, and the next
    /// Engine observation re-adopts it. Cleared on verified unfloat, window
    /// close, and stop; pruned with membership. Sticky, born-fullscreen, and
    /// native overlays keep their own lanes and never enter here.
    floated: BTreeSet<crate::workspace::WindowKey>,
    /// Sticky-float members by stable member key to their pre-sticky float
    /// state (`true` when the window was already a normal float before
    /// sticky-on). Single runtime map, never redundant sets. Sticky rides the
    /// Engine as a slotless float plus this map plus a window-lifetime native
    /// marker; the marker (not this map) survives owner restarts.
    sticky: std::collections::BTreeMap<crate::workspace::WindowKey, bool>,
    /// Observation-scoped write barrier for restart-hydration float
    /// candidates: exact marker/lifetime-validated staged ordinary/sticky
    /// floats whose hydration commit was rejected (or never assembled) this
    /// observation. Rebuilt by every admission preamble (cleared, then
    /// repopulated only for the current observation's rejected staged sets),
    /// keyed by full window lifetime identity, pruned with membership. Row
    /// assembly rides held keys floating and the write gate excludes them,
    /// so rejected candidates take no tiled writes before the next
    /// observation revalidates. Never an adapter float lane: durable
    /// markers are untouched, and success/invalid markers leave no entry.
    restart_reject_hold: BTreeSet<crate::workspace::WindowKey>,
    /// Unconfirmed workspace-domain releases (toggle edges). Retried on
    /// topology edges only (never polled) until the exact domain release
    /// confirms; keyed by domain with latest-intent-wins.
    pending_releases: crate::workspace::PendingReleases,
    /// Topology fingerprint of the last pending-release retry attempt: equal
    /// fingerprints skip silently, so quiet ticks never poll or log.
    pending_retry_fp: u64,
    /// Explicit fixed-window tile overrides by stable member key (R-SPC-04
    /// D3/D6/D7): settled user tile wins for the same live client. The
    /// Engine records the win transactionally at unfloat; this set plus the
    /// on-window `OmniTiler.TileOverride.v1` marker carry it across domain
    /// releases (workspace enable re-adopts via the stamped suppress signal)
    /// and owner restarts. Cleared on re-float, sticky-off-to-float, verified
    /// close, and membership prune; pruned with membership otherwise.
    ///
    /// The runtime set holds EVERY settled explicit win (generic non-fixed
    /// unfloats included): core needs the suppress signal for all of them.
    /// Only the fixed-origin subset below is durable (marker-backed).
    fixed_tile_overrides: BTreeSet<crate::workspace::WindowKey>,
    /// Durable fixed-origin tile overrides by stable member key (R-SPC-04
    /// D7): the subset of [`TileLoop::fixed_tile_overrides`] whose prior
    /// provenance was known automatic fixed at win time (Engine automatic
    /// mark, D4 latch, hydrated fixed override, committed core override, or
    /// a live tile-override marker). Only this set installs the on-window
    /// marker and only it skips the admission clear, reserves the maximized
    /// slot, and refuses automatic origin. Generic non-fixed wins stay in
    /// the runtime set only and keep the existing clear/overlay rules. Hint
    /// loss and predicate changes never erase entries; refloat, verified
    /// close, and membership prune clear them with the runtime set.
    fixed_origin_overrides: BTreeSet<crate::workspace::WindowKey>,
    /// Adapter-asserted automatic fixed-float origins by stable member key
    /// (R-SPC-04 D4): first-seen maximized windows whose fresh tracks are
    /// fixed under the live predicate. They admit slotless and ride floating
    /// beneath native maximize with no clear and no reserved tile slot; the
    /// stamped origin lets the Engine mark them automatic. Lifetime-bound,
    /// pruned with membership; cleared on explicit unfloat (which records an
    /// override instead) and re-float.
    fixed_auto_seen: BTreeSet<crate::workspace::WindowKey>,
    /// First born-fullscreen exits by stable member key (R-SPC-04 D5): the
    /// exit tick classifies with current hints and predicate before any
    /// maximize-clear/restore/slot side effect. One-shot: the next
    /// maximize-clear pass consumes the entry (fixed exits float automatic
    /// and skip the clear via the Engine mark; non-fixed exits tile and take
    /// the normal one-shot clear). Pruned with membership.
    born_first_exit: BTreeSet<crate::workspace::WindowKey>,
    /// Live native overlay flags per Engine token (`(maximized,
    /// fullscreen)`), refreshed on every observation from observed facts
    /// plus retained rows. Feeds the fixed-size admission stamps on Engine
    /// events; keyed by token (never HWND alone) so recycled HWNDs never
    /// inherit, pruned with membership. Sticky/maximized Engine flags stay
    /// adapter-owned (false): their lanes are unchanged by this delivery.
    overlay_now: HashMap<String, (bool, bool)>,
    /// Dedicated-input bridge for the product path (`None` on proof paths,
    /// which keep same-thread hooks). Carries the pump channels plus the
    /// owner-side generation/ack/carry bookkeeping; all owner control calls
    /// route through it so nothing touches input TLS from this thread.
    input_bridge: Option<InputBridge>,
    /// Dedicated-render bridge for the product path (`None` on proof paths,
    /// which keep same-thread overlays, and when the renderer spawn fails).
    /// Carries the render channels plus the owner-side generation/applied
    /// bookkeeping; all owner overlay calls route through it so nothing
    /// touches renderer windows from this thread.
    render_bridge: Option<RenderBridge>,
    /// Bounded per-carrier routine-frame aggregator for render diagnostics:
    /// routine successful repaints aggregate into ~1s per-carrier windows
    /// while transitions and failures log immediately. Runs in both trace and
    /// production modes so geometry churn never floods per frame.
    render_frames: RenderFrameHealth,
}

/// Production snapshot-offer directive (owner publication seam): whether
/// the per-tick candidate reaches the sender. `Retry(g)` resends whatever
/// the current desired content is (even a revert to `last`) with the armed
/// generation `g`; `Fresh` offers normally; `Skip` sends nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotOffer {
    Skip,
    Fresh,
    Retry(u64),
}

/// Decide the per-tick snapshot offer. A refused policy offer stays live as
/// `retry_gen` and resends with its armed generation instead of early
/// returning (a revert to `last_snapshot` would otherwise leave the armed
/// fence pending forever) and instead of bumping a new generation (which
/// would inflate generations on every stalled tick). A pending
/// clear-driven `force` offers even unchanged content so cleared origins
/// republish deterministically.
pub(crate) fn plan_snapshot_offer(
    last: Option<&SharedSnapshot>,
    candidate: &crate::input_handoff::InputSnapshot,
    retry_gen: Option<u64>,
    force: bool,
) -> SnapshotOffer {
    if let Some(generation) = retry_gen {
        return SnapshotOffer::Retry(generation);
    }
    if !force
        && let Some(last) = last
        && snapshot_content_eq(last, candidate)
    {
        return SnapshotOffer::Skip;
    }
    SnapshotOffer::Fresh
}

/// Armed prompt-restore wake: the restored member by full identity plus the
/// Engine token, so a recycled HWND can never count as completion proof.
struct RestoreWake {
    key: crate::workspace::WindowKey,
    token: String,
    until: Instant,
}

#[cfg(test)]
#[path = "tests_esc.rs"]
mod esc_sequence_tests;

#[cfg(test)]
#[path = "tests_input_snapshot.rs"]
mod input_snapshot_offer_tests;

mod scope_effects;

mod scope_sends;

mod scope_trailing;

mod scope_hotplug;

mod monitors;
mod shell;
mod tray_select;

mod observe;

mod hints;

mod pending_seams;

mod placement;

mod minmax;

mod trace;

mod owner_work;

mod preview;

mod input;

mod render_bridge;

mod render_ack;

mod retained;

mod focus;

mod group_underlay;

mod drag_preview;

mod probes;

mod fullscreen;

mod admission;

mod rows;

mod reconcile;

mod geometry;

mod focus_actuation;

mod keyboard_tick;

mod keyboard_directional;

mod keyboard_overlays;

mod keyboard_resize;

mod chord_resolution;

mod float_toggle;

mod dispatch;

mod directional_resolution;

mod directional_cross;

mod output_send;

mod cross_drag;

mod diag;

mod visibility_diag;

mod restart;

mod assignments;

mod workspace_mode;

mod selection;

mod send_preflight;

mod send_native;

mod sends;

mod send_follow;

mod send_stay;

mod history;

mod action_context;

mod output_sync;

mod pager_host;

mod cli_resize;

mod cli_workspace;

mod workspace_navigation;

mod workspace_send_tick;

mod migration;

mod migration_run;

mod migration_visibility;

mod windrag;

mod pending_drop;

mod gesture_tick;

mod settings;

mod run_loop;
mod run_loop_intake;
mod run_loop_tick;
mod run_loop_wait;

mod commands;

mod first_run;

mod proof_commands;

mod inspection;

mod observation;

#[cfg(test)]
#[path = "tests_scope.rs"]
mod tiling_sys_scope_tests;

#[cfg(test)]
#[path = "tests_scope_send.rs"]
mod tiling_sys_scope_send_tests;

#[cfg(test)]
#[path = "tests_scope_hotplug.rs"]
mod tiling_sys_scope_hotplug_tests;

#[cfg(test)]
#[path = "tests_same_axis.rs"]
mod same_axis_move_tests;

#[cfg(test)]
#[path = "tests_fixed_predicate.rs"]
mod fixed_predicate_tests;

#[cfg(test)]
#[path = "tests_born_maximized.rs"]
mod born_maximized_tests;

#[cfg(test)]
#[path = "tests_float_unfloat.rs"]
mod float_unfloat_clear_tests;

#[cfg(test)]
#[path = "tests_owned_exit.rs"]
mod owned_exit_tests;

#[cfg(test)]
#[path = "tests_send_preflight.rs"]
mod send_preflight_tests;

#[cfg(test)]
#[path = "tests_float_send_gate.rs"]
mod float_send_gate_tests;

/// D16 deferred-send production-route regressions (offline, native-free):
/// the real `workspace_do_send` / `workspace_send_follow` /
/// `workspace_do_select` routes with scripted observation, lifetime tags,
/// and apply verdicts. Native hide/focus setters fail closed on the fake
/// HWNDs (honest non-ok outcomes), so these tests pin routing evidence:
/// apply counts, membership transfer, and float-intent retention. Drifted
/// survivor frames prove the skip holds even with drift; ordinary tiled
/// sends prove the applications still run.
#[cfg(test)]
#[path = "tests_deferred_send.rs"]
mod deferred_send_route_tests;

#[cfg(test)]
#[path = "tests_border_shell.rs"]
mod border_shell_tests;

#[cfg(test)]
#[path = "tests_owner_work.rs"]
mod owner_work_trace_tests;

#[cfg(test)]
#[path = "tests_render_frame.rs"]
mod render_frame_tests;

#[cfg(test)]
#[path = "tests_preview.rs"]
mod preview_tests;

#[cfg(test)]
#[path = "tests_rmax03.rs"]
mod rmax03_adapter_tests;

#[cfg(test)]
#[path = "tests_timeout_fallback.rs"]
mod timeout_fallback_tests;

#[cfg(test)]
#[path = "tests_window_diag.rs"]
mod window_diag_tests;

#[cfg(test)]
#[path = "tests_visibility_diag.rs"]
mod visibility_diag_tests;

#[cfg(test)]
#[path = "tests_d23_minimize.rs"]
mod d23_minimize_adapter_tests;

#[cfg(test)]
#[path = "tests_float_nav.rs"]
mod float_nav_adapter_tests;

#[cfg(test)]
#[path = "tests_restart.rs"]
mod restart_adoption_tests;

#[cfg(test)]
#[path = "tests_orientation.rs"]
mod orientation_route_tests;

#[cfg(test)]
#[path = "tests_windrag.rs"]
mod windrag_press_focus_tests;

#[cfg(test)]
#[path = "tests_intent_wake.rs"]
mod intent_wake_tests;

#[cfg(test)]
#[path = "tests_launch.rs"]
mod launch_dispatch_tests;

#[cfg(test)]
/// Stage 3 combined transport/fault stress over the real production
/// primitives (no TileLoop, no threads, no native calls): the input
/// sender stall/escalation contract and the render latest-wins/safety
/// contract hold simultaneously, pinning the suspend-path dual handoff
/// (infallible input clears plus all-carrier render hides with no stale
/// show presentable afterwards).
#[path = "tests_stage3.rs"]
mod stage3_combined_tests;

#[cfg(test)]
#[path = "tests_pending_drop.rs"]
mod pending_drop_tests;

#[cfg(test)]
#[path = "tests_directional_cross.rs"]
mod directional_cross_adapter_tests;

#[cfg(test)]
#[path = "tests_output_send.rs"]
mod output_send_adapter_tests;

#[cfg(test)]
#[path = "tests_cross_drag.rs"]
mod cross_drag_adapter_tests;

#[cfg(test)]
#[path = "tests_workspace_migrate.rs"]
mod workspace_migrate_adapter_tests;
