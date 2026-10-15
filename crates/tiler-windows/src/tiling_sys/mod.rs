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
#[cfg(test)]
use self::cross_drag::prepare_cross_drag_event;
use self::cross_drag::{
    CrossDragSettle, DropDestination, adopt_gaps_for_route, adopt_gaps_on_candidate,
    assemble_cross_drag_rows, cross_topology_fresh, fresh_cross_areas, resolve_drop_destination,
    settle_cross_drag_drop, snapshot_cross_drag,
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
use self::fullscreen::{FULLSCREEN_STYLE_BITS, OwnedExitEffects, exit_fullscreen_owned_with};
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
use self::hints::{HintCx, HintOutcome, HintStats};
#[cfg(test)]
use self::hints::{clear_test_hints, inject_test_hints};
use self::history::{
    log_workspace_action, move_float_to_output, move_overlay_to_output, remap_placement_to_output,
    workspace_history_tick, workspace_log,
};
use self::input::{
    InputBridge, log_input_faults, offer_input_control, offer_input_snapshot, poll_input_ack,
};
use self::keyboard_resize::dispatch_resize_intent;
use self::keyboard_tick::keyboard_tick;
use self::migration::workspace_migrate_tick;
#[cfg(test)]
use self::migration::{migrate_rows_for, workspace_migrate_one};
#[cfg(test)]
use self::migration_run::migration_follow_tail;
use self::migration_run::run_workspace_migrate;
use self::minmax::{fixed_birth_for_slotless, hidden_hint_for, min_hint_for};
pub(crate) use self::monitors::ensure_pm_v2;
use self::monitors::{MonitorArea, all_monitors, monitor_fulls};
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
    DIALOG_CLASS, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, exe_file_name, is_border_shell_popup,
    is_shell_class, rect_from_win,
};
use self::trace::{log_engine_placement_trace, log_json_at, refresh_border_timed};
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

unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<isize>) };
    out.push(hwnd as isize);
    1
}

/// Enumerate top-level HWNDs. `None` on API failure: the caller retains Engine
/// state and skips the tick, never treating a failed enumeration as a
/// complete (possibly empty) observation.
fn enumerate_hwnds() -> Option<Vec<isize>> {
    let mut out: Vec<isize> = Vec::new();
    let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<isize> as LPARAM) };
    if ok == 0 { None } else { Some(out) }
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    Some((point.x, point.y))
}

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

impl TileLoop {
    fn correlation(&self) -> CorrelationId {
        CorrelationId::parse(&format!("tick-{}", self.tick))
            .expect("tick correlation is a valid token")
    }

    /// D23 hidden-iconic refresh at the observation boundary:
    /// `HiddenRecord.iconic` freezes at hide time, but a hidden window can
    /// minimize after hide while the raw gate skips it as invisible, which
    /// would otherwise retain its leaf forever. For owned hidden
    /// exact-lifetime members (stored tag reads live, the same bar as the
    /// hide effects), refresh the per-tick omission from the native iconic
    /// state (read-only; fixture seam under test). Only enumerated HWNDs
    /// refresh: destroyed or absent evidence stays with the audit, and
    /// unreadable evidence never fabricates a row either way. Visible
    /// iconic members omit via the real arm below (re-insertion here would
    /// be idempotent); the hide-time record itself is never rewritten, so
    /// the reveal preimage stays intact.
    fn refresh_hidden_iconic(&mut self) {
        let keys: Vec<crate::workspace::WindowKey> = self.hidden_claims.keys().cloned().collect();
        for key in keys {
            if !self.last_hwnds.contains(&key.hwnd) {
                continue;
            }
            if !self.member_tokens.contains_key(&key) {
                continue;
            }
            let owned = self.member_tags.get(&key).is_some_and(|stored| {
                !stored.is_empty()
                    && crate::workspace_owner::visible_lifetime_ok(
                        stored,
                        live_member_tag(key.hwnd).as_deref(),
                    )
            });
            if !owned {
                continue;
            }
            if live_is_iconic(key.hwnd) {
                self.minimized_omitted.insert(key);
            }
        }
    }

    /// Full enumeration with per-window eligibility. Proof mode prefilters by
    /// frozen-allowlist HWND before any per-window query, so non-owned windows
    /// never mint tokens, churn the map, or enter logs; `EnumWindows` still
    /// yields the complete raw count. Proof mode additionally requires
    /// frozen-allowlist membership plus owned-helper verification (sibling
    /// exe/class/lifetime-tag); malformed allowlist state is unreachable here
    /// because `cmd_tile_proof` refuses it before the loop starts. Invisible
    /// allowlist members stay frozen (not eligible) until an exact-bound
    /// `show` admission. Normal mode with a nonempty scope additionally
    /// reports out-of-scope executables as `scope-excluded` with no
    /// membership, geometry, or focus. Returns `None` when enumeration itself
    /// failed: the tick is skipped with retained Engine state.
    fn observe(
        &mut self,
        me: &ProcessIdentity,
        fulls: &[Rect],
        skipped: &mut Vec<(String, String)>,
        retained: &mut Vec<RetainedRow>,
    ) -> Option<Vec<ObservedWindow>> {
        #[cfg(test)]
        if let Some((observed, retained_injected, enum_failed)) =
            INJECTED_OBSERVE.with(|slot| slot.borrow().clone())
        {
            if enum_failed {
                return None;
            }
            // D23 injected fixture contract: iconic HWNDs join the raw
            // inventory (production enumeration yields iconic windows).
            // Visible iconic members resolve to omission exactly as the real
            // iconic arm does; hidden members refresh through the owned
            // exact-lifetime gate below (never unconditional). Unreadable
            // HWNDs join the inventory only (production `Unknown`: present,
            // never a close, never a row). The omission set refreshes every
            // pass, never leaking across ticks. Fixtures without
            // iconic/unreadable HWNDs behave exactly as before.
            self.minimized_omitted.clear();
            let iconic: std::collections::HashSet<u64> =
                INJECTED_ICONIC.with(|slot| slot.borrow().clone());
            let unreadable: std::collections::HashSet<u64> =
                INJECTED_UNREADABLE.with(|slot| slot.borrow().clone());
            self.last_enumerated =
                observed.len() + retained_injected.len() + iconic.len() + unreadable.len();
            self.last_hwnds = observed
                .iter()
                .map(|window| window.hwnd)
                .chain(retained_injected.iter().map(|row| row.key.hwnd))
                .chain(iconic.iter().copied())
                .chain(unreadable.iter().copied())
                .collect();
            for hwnd in &iconic {
                if self.hidden_claims.keys().any(|key| key.hwnd == *hwnd) {
                    continue;
                }
                if let Some(key) = self
                    .member_tokens
                    .keys()
                    .find(|key| key.hwnd == *hwnd)
                    .cloned()
                {
                    self.minimized_omitted.insert(key);
                }
            }
            // Same hidden-iconic refresh as the real path below, driven by
            // the fixture seam instead of native queries.
            self.refresh_hidden_iconic();
            retained.extend(retained_injected);
            let _ = (me, fulls);
            return Some(observed);
        }
        use ObserveFailure::{Known, Unknown};
        let proof_mode = self.allowlist.is_some();
        let hwnds = enumerate_hwnds()?;
        self.last_enumerated = hwnds.len();
        self.last_hwnds = hwnds.iter().map(|raw| *raw as usize as u64).collect();
        // D23: fresh omission set per pass; stale minimized keys must never
        // leak into the next tick's row assembly. Hidden-iconic refresh
        // runs on the enumerated inventory before any per-window query.
        self.minimized_omitted.clear();
        self.refresh_hidden_iconic();
        let mut out = Vec::new();
        let mut unreadable = 0usize;
        // Every resolved identity pair mints or reuses a token; retain all of
        // them below (eligible, skipped, and frame-failed) so known windows
        // keep stable tokens instead of churning one new token per tick.
        let mut enumerated: Vec<(u64, String)> = Vec::new();
        for raw in hwnds {
            // The process-owned border overlay is never a tile target, a
            // token source, or an unreadable count: skip it before any query.
            if owned_overlay_hwnd(self, RenderCarrier::Border) == Some(raw as usize as u64) {
                continue;
            }
            // Same for the process-owned group-underlay fill: never a tile
            // target, so its own move/paint events cannot retarget tiling or
            // the underlay itself.
            if owned_overlay_hwnd(self, RenderCarrier::Underlay) == Some(raw as usize as u64) {
                continue;
            }
            let hwnd = raw as HWND;
            if proof_mode {
                // Frozen-HWND prefilter before any per-window query.
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                if !entries
                    .iter()
                    .any(|entry| entry.hwnd == raw as usize as u64)
                {
                    continue;
                }
            }
            let window = match observe_window(hwnd, me, fulls, &mut self.tokens) {
                Ok(window) => window,
                Err(Known(known, reason)) => {
                    // Identity known: retain it and skip with the accurate
                    // reason (minimized needs no frame; unreadable keeps its
                    // stable token instead of counting as changed identity).
                    // D23 minimize release: a minimized window omits its
                    // Engine row (no retained-occupancy allocation) so
                    // convergence releases the leaf, reflows survivors,
                    // resolves MRU-or-none focus, and stores the
                    // `MinimizedSlot` for best-effort rejoin. The token stays
                    // stable via `enumerated` below, so a native restore
                    // re-carries the same Engine identity purely tiled.
                    // Out-of-scope executables never retain membership.
                    enumerated.push((known.identity.hwnd, known.identity.process_creation.clone()));
                    if !scope_allows(&self.scope, &known.identity.exe_path) {
                        skipped.push((known.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    skipped.push((known.token.clone(), reason.as_str().to_owned()));
                    if reason == SkipReason::Minimized {
                        self.minimized_omitted.insert(crate::workspace::WindowKey {
                            hwnd: known.identity.hwnd,
                            pid: known.identity.pid,
                            creation: known.identity.process_creation.clone(),
                        });
                        continue;
                    }
                    retained.push(RetainedRow {
                        key: crate::workspace::WindowKey {
                            hwnd: known.identity.hwnd,
                            pid: known.identity.pid,
                            creation: known.identity.process_creation.clone(),
                        },
                        token: known.token.clone(),
                        rect: None,
                        maximized: false,
                        fullscreen: false,
                        facts: None,
                    });
                    continue;
                }
                Err(Unknown) => {
                    unreadable += 1;
                    continue;
                }
            };
            enumerated.push((window.hwnd, window.identity.process_creation.clone()));
            if proof_mode {
                let entries = self.allowlist.as_ref().expect("proof mode has allowlist");
                let Some(entry) = entries
                    .iter()
                    .find(|entry| allow_match(entry, &window.identity))
                else {
                    continue;
                };
                // Owned-helper gate: non-owned windows never enter management
                // even when otherwise eligible. Invisible frozen members are
                // still verified (passive admission) but stay skipped until
                // shown.
                if verify_proof_owned(window.hwnd, entry, me).is_err() {
                    skipped.push((window.token.clone(), "identity-changed".to_owned()));
                    continue;
                }
            }
            match classify(&window.facts) {
                Ok(()) => {
                    if !scope_allows(&self.scope, &window.identity.exe_path) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        continue;
                    }
                    // Listed hosts pass only with a live matching hosted
                    // child, re-verified every tick: a newly appearing hosted
                    // app can never ride another app's host executable into
                    // observation, membership, writes, or focus. Excluded
                    // members keep their retained snapshot (no membership
                    // loss) and re-enter when the child set is clean again.
                    if !hosted_gate_allows(
                        &window.identity.exe_path,
                        window.hwnd,
                        window.identity.pid,
                        &self.scope_hosts,
                    ) {
                        skipped.push((window.token.clone(), "scope-excluded".to_owned()));
                        retained.push(RetainedRow {
                            key: crate::workspace::WindowKey {
                                hwnd: window.identity.hwnd,
                                pid: window.identity.pid,
                                creation: window.identity.process_creation.clone(),
                            },
                            token: window.token.clone(),
                            rect: None,
                            maximized: false,
                            fullscreen: false,
                            facts: None,
                        });
                        continue;
                    }
                    out.push(window);
                }
                Err(reason) => {
                    skipped.push((window.token.clone(), reason.as_str().to_owned()));
                    // Ineligible but fully observed: the fresh frame keeps
                    // retained occupancy (maximized, fullscreen, cloaked)
                    // inside Engine membership with no geometry writes. The
                    // fresh overlay flags ride along so move/pointer routes
                    // refuse before mutation and admission clears once.
                    retained.push(RetainedRow {
                        key: crate::workspace::WindowKey {
                            hwnd: window.hwnd,
                            pid: window.identity.pid,
                            creation: window.identity.process_creation.clone(),
                        },
                        token: window.token.clone(),
                        rect: Some(window.visible),
                        maximized: window.facts.maximized,
                        fullscreen: window.facts.captionless_fullscreen,
                        facts: Some(window.facts),
                    });
                }
            }
        }
        // Bounded retain over every enumerated pair, never HWND alone and
        // never eligible-only: skipped identities keep their tokens. Known
        // hidden claims are retained too: they never pass the visible gate
        // but their tokens stay stable across ticks.
        let mut live_refs: Vec<ObservedTargetRef<'_>> = enumerated
            .iter()
            .map(|(hwnd, creation)| ObservedTargetRef {
                hwnd: *hwnd,
                creation,
            })
            .collect();
        for record in self.hidden_claims.values() {
            if !live_refs.iter().any(|t| {
                t.hwnd == record.claim.hwnd && t.creation == record.claim.process.process_creation
            }) {
                live_refs.push(ObservedTargetRef {
                    hwnd: record.claim.hwnd,
                    creation: &record.claim.process.process_creation,
                });
            }
        }
        self.tokens.retain(&live_refs);
        if unreadable > 0 {
            skipped.push((
                format!("unreadable-{unreadable}"),
                SkipReason::Unreadable.as_str().to_owned(),
            ));
        }
        track_fullscreen_holds(self, &out, retained);
        // Live native overlay snapshot per Engine token for the fixed-size
        // admission stamps: observed facts plus retained rows, refreshed on
        // every observation. Stale entries prune below by live membership;
        // recycled HWNDs mint fresh tokens and never inherit.
        for window in &out {
            self.overlay_now.insert(
                window.token.clone(),
                (window.facts.maximized, window.facts.captionless_fullscreen),
            );
        }
        for row in retained.iter() {
            self.overlay_now
                .insert(row.token.clone(), (row.maximized, row.fullscreen));
        }
        let live_tokens: std::collections::HashSet<&String> = out
            .iter()
            .map(|w| &w.token)
            .chain(retained.iter().map(|r| &r.token))
            .collect();
        self.overlay_now
            .retain(|token, _| live_tokens.contains(token));
        Some(out)
    }

    fn focused_token(&self, observed: &[ObservedWindow]) -> Option<WindowId> {
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        observed
            .iter()
            .find(|w| w.hwnd == foreground)
            .map(|w| WindowId(w.token.clone()))
    }
}

/// `tile` command: normal user tiling only (explicit `--user-start`).
/// Geometry is left in place on stop; nothing is hidden or restored.
/// Terminal windows are ordinary targets; agents never run this path.
/// Optional repeatable `--scope-exe NAME` restricts management to the named
/// executables (empty default means no filter). Product keyboard takeover
/// follows the parsed options (default on, visible off switch, Win+L
/// opt-in). Session-only mouse-Snap prevention is default on with the
/// visible `--no-mouse-snap-prevention` off switch; the exact preimage is
/// restored conditionally on stop.
pub fn cmd_tile(
    options: &TileOptions,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    cli_overrides: crate::tiling::CliOverrides,
) -> Result<String> {
    if !options.user_start {
        return Err(err("refuse: tile requires explicit --user-start"));
    }
    // The first-run preset resolves under the owner lease inside the loop
    // below (never before it): a refused second owner never prompts, and a
    // mid-prompt file race reloads instead of overwriting. These pre-lease
    // values are the no-prompt fallback; the lease-held resolution may
    // replace every derived lane before any effect or hook.
    let trace = options.trace;
    let seconds = options.seconds;
    let keyboard = KeyboardConfig {
        takeover: !options.no_keyboard_snap_takeover,
        allow_win_l: options.allow_win_l,
    };
    let mouse_snap = !options.no_mouse_snap_prevention;
    let scope = options.scope_exes.clone();
    let scope_hosts = options.scope_hosts.clone();
    let border = options.border;
    let underlay = options.underlay;
    let inner_gap = options.inner_gap;
    let outer_gap = options.outer_gap;
    let options = options.clone();
    let pending = FirstRunPending {
        options,
        live: live.clone(),
        settings_dir: settings_dir.clone(),
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: None,
                proof: false,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv: Vec::new(),
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: Some(pending),
                inner_gap,
                outer_gap,
                settings_dir,
                settings_live: live,
                cli_overrides,
                scope,
                scope_hosts,
                border,
                underlay,
            },
        )
    })
}

/// Deferred first-run inputs for the normal `tile` owner only. The parsed
/// options plus the pre-lease settings state travel into the owned loop and
/// resolve under the lease before any effect or hook.
struct FirstRunPending {
    options: TileOptions,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
}

/// Resolved first-run state for the normal `tile` owner: the effective
/// options plus the live settings state, directory, and `tile-start` note.
#[derive(Debug, Clone)]
pub struct FirstRunResolution {
    pub options: TileOptions,
    pub live: Option<LiveSettings>,
    pub settings_dir: Option<std::path::PathBuf>,
    pub first_run: Option<String>,
}

/// Carry the CLI/run-only lanes over a settings base: explicit CLI switches
/// re-apply over the base, and seconds/trace/scope plus the user-start fence
/// (never from the file) carry over from the parsed options.
fn options_with_cli(
    base_settings: &crate::settings::Settings,
    cli_overrides: &crate::tiling::CliOverrides,
    run_options: &TileOptions,
) -> TileOptions {
    let mut base = crate::settings::tile_options_from_settings(base_settings);
    crate::tiling::apply_cli_overrides(&mut base, cli_overrides);
    base.seconds = run_options.seconds;
    base.trace = run_options.trace;
    base.user_start = run_options.user_start;
    base.scope_exes = run_options.scope_exes.clone();
    base.scope_hosts = run_options.scope_hosts.clone();
    base
}

/// Authoritative base when the prompt choice is discarded (a file appeared
/// mid-prompt, the create race was lost, or the choice failed validation):
/// the file wins when it loads, otherwise last-good defaults stay live with
/// a degraded status. Explicit CLI switches and run-only lanes re-apply over
/// either base. Nothing is written.
fn authoritative_base(
    dir: &std::path::Path,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
) -> (TileOptions, Option<LiveSettings>) {
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = options_with_cli(&settings, cli_overrides, run_options);
            (options, Some(LiveSettings::fresh(settings, mtime)))
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            let options = options_with_cli(&live.settings.clone(), cli_overrides, run_options);
            (options, Some(live))
        }
    }
}

/// Lease-held first-run resolution for the normal `tile` owner only. The
/// caller holds the single-owner lease, so a second owner never reaches
/// here (its lease refusal lands before any UI). A stop racing startup
/// skips the modal UI and the loop exits promptly; a stop arriving while the
/// prompt is pending dismisses it and publishes nothing. After the choice,
/// the file is rechecked: a file that appeared mid-prompt discards the choice
/// and reloads the authoritative base without writing. An absent file
/// persists through the atomic create-if-absent publish: the winner's bytes
/// land complete, a lost race discards the choice and reloads instead of
/// overwriting, and a plain I/O failure keeps the choice in memory for this
/// run only with an explicit unsaved status plus a `first-run-save-failed`
/// line (never a silent success claim).
fn resolve_first_run(
    dir: &Path,
    me: &ProcessIdentity,
    pending: FirstRunPending,
    cli_overrides: &crate::tiling::CliOverrides,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let FirstRunPending {
        options,
        live,
        settings_dir,
    } = pending;
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    let Some(sdir) = settings_dir.clone() else {
        return Ok(done(options, live, settings_dir, None));
    };
    let file_missing_before = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    if !crate::tray::first_run_should_prompt(file_missing_before, stop_requested(dir, me)?) {
        return Ok(done(options, live, settings_dir, None));
    }
    // Owned cancellable dialog under the lease: a graceful stop while pending
    // destroys the owned window (stop wins) and publishes nothing below.
    let outcome = crate::tray_sys::first_run_dialog(|| stop_requested(dir, me).unwrap_or(false));
    let stop_during = stop_requested(dir, me)?;
    let file_missing_after = matches!(
        crate::settings::load_from_dir(&sdir),
        crate::settings::LoadOutcome::Missing
    );
    resolve_first_run_outcome(
        &sdir,
        outcome.choice(),
        file_missing_after,
        stop_during,
        &options,
        cli_overrides,
        live,
        settings_dir,
        log_path,
    )
}

/// Lease-held first-run outcome application: the write-capable production
/// path shared by the owner and offline tests. `choice` is the dialog result
/// (`None` when dismissed), `file_missing_after` the lease-held recheck, and
/// `stop_during` the lease-held stop recheck. A stop wins over any pending
/// choice (nothing published); a mid-prompt file discards the choice without
/// overwriting; an absent file publishes the choice through the atomic
/// create-if-absent path. Every branch reports its `tile-start` note.
#[allow(clippy::too_many_arguments)]
pub fn resolve_first_run_outcome(
    sdir: &Path,
    choice: Option<crate::tray::FirstRunChoice>,
    file_missing_after: bool,
    stop_during: bool,
    run_options: &TileOptions,
    cli_overrides: &crate::tiling::CliOverrides,
    live: Option<LiveSettings>,
    settings_dir: Option<std::path::PathBuf>,
    log_path: &Path,
) -> Result<FirstRunResolution> {
    let done = |options, live, settings_dir, first_run| FirstRunResolution {
        options,
        live,
        settings_dir,
        first_run,
    };
    // The pending inputs travel by value: discarding branches rebuild the
    // authoritative base from the file, publishing branches derive from it.
    let options = run_options.clone();
    match crate::tray::first_run_post_decision(choice, file_missing_after, stop_during) {
        crate::tray::FirstRunPostDecision::CancelledStopped => {
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-cancelled", "reason": "stopped"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:cancelled:stopped".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::DismissedUnsaved => Ok(done(
            options,
            live,
            settings_dir,
            Some("first-run:dismissed:unsaved".to_owned()),
        )),
        crate::tray::FirstRunPostDecision::DiscardPresent => {
            let (options, live) = authoritative_base(sdir, &options, cli_overrides);
            log_json_at(
                log_path,
                serde_json::json!({"event": "first-run-discarded", "reason": "file-appeared"}),
            );
            Ok(done(
                options,
                live,
                settings_dir,
                Some("first-run:discarded:present".to_owned()),
            ))
        }
        crate::tray::FirstRunPostDecision::Publish(choice) => {
            let chosen = crate::tray::settings_for_choice(choice);
            if let Err(reason) = crate::settings::validate_settings(&chosen) {
                let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                log_json_at(
                    log_path,
                    serde_json::json!({"event": "first-run-discarded", "reason": reason.to_string()}),
                );
                return Ok(done(
                    options,
                    live,
                    settings_dir,
                    Some("first-run:discarded:invalid".to_owned()),
                ));
            }
            let bytes = serde_json::to_vec_pretty(&chosen)
                .map_err(|_| err("error: first-run serialize"))?;
            match crate::storage::publish_no_overwrite(
                sdir,
                crate::settings::SETTINGS_FILE_NAME,
                &bytes,
                "first-run",
            ) {
                Ok(()) => {
                    let mtime = std::fs::metadata(sdir.join(crate::settings::SETTINGS_FILE_NAME))
                        .and_then(|meta| meta.modified())
                        .ok();
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    let live = Some(LiveSettings::fresh(chosen, mtime));
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some(format!("first-run:{}:saved", choice.as_str())),
                    ))
                }
                Err(crate::storage::PublishError::Pending) => {
                    let (options, live) = authoritative_base(sdir, &options, cli_overrides);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-discarded", "reason": "save-race"}),
                    );
                    Ok(done(
                        options,
                        live,
                        settings_dir,
                        Some("first-run:discarded:raced".to_owned()),
                    ))
                }
                Err(crate::storage::PublishError::Io(error)) => {
                    let mut fresh = LiveSettings::fresh(chosen.clone(), None);
                    fresh.status = format!("first-run:{}:unsaved", choice.as_str());
                    let options = options_with_cli(&chosen, cli_overrides, &options);
                    log_json_at(
                        log_path,
                        serde_json::json!({"event": "first-run-save-failed", "error": error.to_string()}),
                    );
                    Ok(done(
                        options,
                        Some(fresh),
                        settings_dir,
                        Some(format!("first-run:{}:unsaved", choice.as_str())),
                    ))
                }
            }
        }
    }
}

/// Load the normal-owner settings base: persisted settings supply the
/// `TileOptions` defaults that explicit CLI switches override. Proof owners
/// never call this (deterministic explicit configuration only). Returns the
/// base options plus the initialized last-good live state and directory for
/// pump polling (`None` live state disables polling).
pub fn load_normal_tile_base() -> (
    TileOptions,
    Option<LiveSettings>,
    Option<std::path::PathBuf>,
) {
    let defaults = crate::tiling::tile_options_defaults();
    let Ok(dir) = crate::native::settings_directory() else {
        return (defaults, None, None);
    };
    let mtime = std::fs::metadata(dir.join(crate::settings::SETTINGS_FILE_NAME))
        .and_then(|meta| meta.modified())
        .ok();
    match crate::settings::load_from_dir(&dir) {
        crate::settings::LoadOutcome::Loaded(settings) => {
            let options = crate::settings::tile_options_from_settings(&settings);
            (
                options,
                Some(LiveSettings::fresh(settings, mtime)),
                Some(dir),
            )
        }
        crate::settings::LoadOutcome::Missing => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = "missing: defaults".to_owned();
            (defaults, Some(live), Some(dir))
        }
        crate::settings::LoadOutcome::Invalid(error) => {
            let mut live = LiveSettings::fresh(crate::settings::Settings::default(), mtime);
            live.status = format!("degraded:{error}");
            (defaults, Some(live), Some(dir))
        }
    }
}

/// `workspace` command: exact-owner out-of-hook control for the normal `tile`
/// loop only. Queues one bounded `workspace.request` file; the owner validates
/// the full owner binding (creation/pid/exe/sid/session plus a client
/// correlation) and dispatches once through the existing `workspace_do_select`
/// / `workspace_do_send` resolvers (or the existing project-owned fullscreen
/// toggle for `--fullscreen`). No window actuation here, no synthetic input,
/// acceptance. Refuses when no normal owner runs, when the caller is not the
/// same medium path (SID/session/exe), when the ledger owner is a proof run,
/// and when a request is already pending (single-pending queue, no overwrite,
/// no replay). Stdout reports `dispatched` (queued) honestly; completion is
/// the owner's `workspace` (or `fullscreen-toggle` for `--fullscreen`) log
/// outcome, observed natively by the caller.
pub fn cmd_workspace(options: &WorkspaceOptions) -> Result<String> {
    use crate::tiling::{WORKSPACE_REQUEST_VERSION, WorkspaceRequest, render_workspace_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if options.index > 9 {
        return Err(err("refuse: workspace index must be 0..=9"));
    }
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: workspace request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = WorkspaceRequest {
        v: WORKSPACE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        action: options.action,
        index: options.index,
        direction: options.direction,
        correlation: correlation.clone(),
    };
    let body = render_workspace_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        WORKSPACE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: workspace request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": options.action.as_str(), "index": options.index, "direction": options.direction.map(|d| d.as_str())}).to_string())
}

/// Test-needed exact-owner keyboard-resize control for the normal `tile`
/// loop only (tentative, pending user review; the product hook still filters
/// injected chords, so synthetic input can never drive a resize). Mirrors
/// `cmd_workspace` exactly: same-path medium caller, exact ledger-owner
/// liveness, proof-owner refusal, and a bounded single-pending request file
/// (`resize.request`). The owner consumes the request once, resolves the
/// live foreground as the resize subject through the production origin/
/// member/lifetime/scope gates, and dispatches through the real
/// `keyboard_tick` resize arm: no classifier bypass, no fence bypass. The
/// CLI carries direction/mode only, never an HWND.
pub fn cmd_resize(options: &ResizeOptions) -> Result<String> {
    use crate::tiling::{RESIZE_REQUEST_VERSION, ResizeRequest, render_resize_request};
    ensure_pm_v2()?;
    let me = medium_caller()?;
    // Like `cmd_stop`, the CLI itself may run from any shell; only the
    // ledger owner liveness plus the same-path medium binding gate below.
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    // Same-path medium caller check, mirroring `cmd_stop`: SID, session, exe.
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    // Proof owners never serve the normal control: a proof audit marker for
    // this exact owner creation proves proof mode (normal runs write none).
    let audit = dir.join(format!(
        "proof-audit-{}.jsonl",
        record.owner.process_creation
    ));
    if audit.exists() {
        return Err(err("refuse: proof owner"));
    }
    // The ledger owner must be exactly alive (full identity, no PID reuse).
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let rid = held
        .integrity()
        .map_err(|e| err(format!("error: target integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: target integrity {rid} is not medium")));
    }
    if !crate::tiling::resize_direction_valid(&options.direction) {
        return Err(err("refuse: resize direction must be left|right|up|down"));
    }
    if !crate::tiling::resize_mode_valid(&options.mode) {
        return Err(err("refuse: resize mode must be outwards|inwards"));
    }
    let path = dir.join(RESIZE_REQUEST_FILE);
    if path.exists() {
        return Err(err("refuse: resize request pending"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let correlation = format!("cli-{}-{:x}", me.pid, nanos);
    if tiler_core::ids::CorrelationId::parse(&correlation).is_none() {
        return Err(err("error: correlation render"));
    }
    let request = ResizeRequest {
        v: RESIZE_REQUEST_VERSION,
        creation: record.owner.process_creation.clone(),
        pid: record.owner.pid,
        exe_path: record.owner.exe_path.clone(),
        user_sid: record.owner.user_sid.clone(),
        session_id: record.owner.session_id,
        direction: options.direction.clone(),
        mode: options.mode.clone(),
        correlation: correlation.clone(),
    };
    let body = render_resize_request(&request);
    if body.is_empty() {
        return Err(err("error: request render"));
    }
    // Bounded single-pending queue: the body is written and synced to a
    // same-directory unique temp first and only then published atomically
    // with no-overwrite semantics, so the owner never observes a partial
    // body. A publish race against an already-queued request refuses.
    match crate::storage::publish_no_overwrite(
        &dir,
        RESIZE_REQUEST_FILE,
        body.as_bytes(),
        &correlation,
    ) {
        Ok(()) => {}
        Err(crate::storage::PublishError::Pending) => {
            return Err(err("refuse: resize request pending"));
        }
        Err(crate::storage::PublishError::Io(e)) => {
            return Err(err(format!("error: request write: {e}")));
        }
    }
    Ok(serde_json::json!({"dispatched": true, "op": "resize", "direction": options.direction, "mode": options.mode, "route": "test-needed-tentative"}).to_string())
}

/// `tile-proof` command: owned-helpers-only proof loop. Requires a nonempty
/// valid `--allowlist` at parse and at native start; malformed, empty,
/// duplicate, untagged, or non-owned entries refuse before any lease or write
/// and never fall back to normal mode. The raw received argv travels with the
/// parsed options into the proof-only audit (never production logs) with a
/// parsed/raw consistency check, so flag delivery is evidenced. Every setter
/// carries a proof audit record (requested/native target plus flags/outcome);
/// production logs stay token-only. Proof installs no keyboard hook; automated
/// synthetic-input verification uses the separate `shortcut-proof` command.
pub fn cmd_tile_proof(options: &TileProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig::disabled();
    // Proof installs no keyboard hook and takes no Snap setting: the proof
    // contract is unchanged unless a future explicit opt-in lands.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap: false,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `shortcut-proof` command: owned-helpers-only automated shortcut proof.
/// Same frozen-allowlist geometry gate as `tile-proof` (never falls back to
/// normal), but the owner installs the hook with test-only acceptance of
/// exactly [`crate::snapkey::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` and
/// drives the same keyboard dispatcher (fresh observation per intent, Engine
/// focus/move, native readback) plus the same session-only mouse routines.
/// Unshifted Win+L stays gated off (no flag offers it): live runs must never
/// send Win+L. Raw argv travels into the proof-only audit with a consistency
/// check; production logs stay token-only.
pub fn cmd_shortcut_proof(
    options: &crate::tiling::ShortcutProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_shortcut_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: true,
                workspace_proof: false,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `workspace-proof` command: owned-helpers-only automated workspace proof.
/// Same frozen-allowlist geometry gate as `shortcut-proof` plus the workspace
/// dispatcher (select/send/follow with hide/reveal) for exactly the
/// allowlisted helpers. The hook accepts exactly
/// [`crate::snapkey::SHORTCUT_PROOF_MARKER`]; every workspace hide verifies
/// `verify_proof_owned` fresh before the write. `shortcut-proof` never hides
/// for workspace intents and `tile-proof` installs no hook.
pub fn cmd_workspace_proof(
    options: &crate::tiling::WorkspaceProofOptions,
    raw_argv: &[String],
) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_workspace_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let border = options.border;
    let underlay = options.underlay;
    let mouse_snap = !options.no_mouse_snap_prevention;
    let raw_argv = raw_argv.to_vec();
    let keyboard = KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    crate::lifecycle::sys::run_product(trace, mouse_snap, move |dir, me, store| {
        run_tile_loop(
            dir,
            me,
            store,
            TileRun {
                seconds,
                trace,
                allowlist: Some(entries),
                proof: true,
                shortcut_proof: false,
                workspace_proof: true,
                raw_argv,
                keyboard,
                mouse_snap,
                first_run: None,
                first_run_pending: None,
                inner_gap: INNER_GAP,
                outer_gap: OUTER_GAP,
                settings_dir: None,
                settings_live: None,
                cli_overrides: crate::tiling::CliOverrides::default(),
                scope: Vec::new(),
                scope_hosts: Vec::new(),
                border,
                underlay,
            },
        )
    })
}

/// `hide-proof` command: owned-helpers-only visibility proof over the product
/// nonce mechanism. Requires a nonempty valid `--allowlist`; every frozen
/// entry is verified as an owned helper (sibling exe/class/lifetime-tag via
/// [`verify_proof_owned`]) BEFORE the ordinary product nonce APIs run, so
/// helper-only gates are never weakened. Each verified helper is admitted with a fresh product nonce and hidden with a
/// write-before-hide ledger commit (schema v4) under the central product
/// watcher; graceful stop auto-reveals via the guarded teardown, and
/// emergency loss auto-reveals via the watcher with idempotent independent
/// restore. Installs no hook, takes no Snap setting, moves no geometry.
pub fn cmd_hide_proof(options: &HideProofOptions, raw_argv: &[String]) -> Result<String> {
    let text = std::fs::read_to_string(&options.allowlist)
        .map_err(|e| err(format!("error: allowlist read: {e}")))?;
    let entries = parse_allowlist(&text).map_err(err)?;
    if entries.is_empty() {
        return Err(err("refuse: empty allowlist"));
    }
    crate::tiling::verify_hide_proof_argv_consistency(raw_argv, options).map_err(err)?;
    let trace = options.trace;
    let seconds = options.seconds;
    let raw_argv = raw_argv.to_vec();
    // No Snap prevention, no hook, no geometry: visibility proof only.
    crate::lifecycle::sys::run_product(trace, false, move |dir, me, store| {
        run_hide_proof_loop(dir, me, store, entries, seconds, trace, raw_argv)
    })
}

fn run_hide_proof_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    entries: Vec<AllowEntry>,
    seconds: Option<u64>,
    trace: bool,
    raw_argv: Vec<String>,
) -> Result<()> {
    ensure_pm_v2()?;
    let log_path = log_path_for(dir, &me.process_creation);
    let audit_path = dir.join(format!("proof-audit-{}.jsonl", me.process_creation));
    if audit_path.exists() {
        return Err(err("refuse: audit preexists, will not overwrite"));
    }
    let digest = allowlist_digest(&entries);
    let count = entries.len();
    log_json_at(
        &audit_path,
        serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": "hide-proof",
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Frozen-helper gate BEFORE any product nonce API: every entry must verify
    // as an owned helper, never an ordinary window. Opaque index only in
    // product errors; raw identity stays in the proof audit below.
    for (index, entry) in entries.iter().enumerate() {
        verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
            err(format!(
                "refuse: hide-proof allowlist non-owned index={index} {reason}"
            ))
        })?;
    }
    let frozen: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "hwnd": e.hwnd,
                "pid": e.pid,
                "process_creation": e.process_creation,
                "exe_path": e.exe_path,
                "user_sid": e.user_sid,
                "session_id": e.session_id,
                "tag": e.tag,
            })
        })
        .collect();
    log_json_at(
        &audit_path,
        serde_json::json!({"event": "proof-frozen", "windows": frozen}),
    );
    let areas = all_monitors()?;
    let monitor = areas[0].clone();
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "hide-proof-start",
            "mode": "hide-proof",
            "trace": trace,
            "monitors": areas.len(),
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "allowlist_digest": digest,
            "allowlist_count": count,
        }),
    );
    // Admit + hide each verified helper via the ordinary product APIs.
    // Write-before-hide holds per window: the ledger commit precedes the
    // hide. Opaque index only in product errors; raw identity stays in the
    // proof audit below.
    let mut hidden = 0usize;
    let mut admitted: Vec<crate::model::WindowIdentity> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let claim = crate::product_hide::sys::admit_product_target(entry.hwnd, me)
            .map_err(|e| err(format!("error: hide-proof admit index={index} {e}")))?;
        crate::product_hide::sys::hide_committed_product(store, me, &claim, dir)
            .map_err(|e| err(format!("error: hide-proof hide index={index} {e}")))?;
        admitted.push(claim);
        hidden += 1;
        log_json_at(
            &log_path,
            serde_json::json!({"event": "hide-proof-hide", "index": hidden, "count": count}),
        );
        log_json_at(
            &audit_path,
            serde_json::json!({
                "event": "hide-proof-hide",
                "hwnd": entry.hwnd,
                "pid": entry.pid,
                "process_creation": entry.process_creation,
                "tag": entry.tag,
            }),
        );
    }
    // Ledger receipt check: current version with exactly the admitted product
    // claims, verified field-by-field (kind, HWND, full process, tag), not by
    // count alone, so a substituted claim cannot pass the receipt.
    match store.committed() {
        Ok(Some(record)) => {
            let exact = record.v == crate::model::LEDGER_SCHEMA_VERSION
                && record.windows.len() == admitted.len()
                && admitted.iter().all(|claim| {
                    record.windows.iter().any(|w| {
                        w.kind == crate::model::WindowClaimKind::Product
                            && w.hwnd == claim.hwnd
                            && w.process == claim.process
                            && w.tag == claim.tag
                    })
                });
            if !exact {
                return Err(err("error: hide-proof ledger receipt mismatch"));
            }
        }
        _ => return Err(err("error: hide-proof ledger receipt missing")),
    }
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    loop {
        if stop_requested(dir, me)? {
            return Ok(());
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// `capture` command: read-only frozen-allowlist capture of explicitly listed
/// owned-helper windows only. Each `--hwnd` target is fully validated
/// (owned-helper identity with lifetime tag, proof-mode eligibility, fresh
/// rectangles from one coherent observe pair); any ineligible or untagged
/// target refuses the whole capture. The HWND pid is rechecked immediately
/// before the receipt write. Receipts may carry raw native ids; production
/// logs never do.
pub fn cmd_capture(options: &CaptureOptions) -> Result<String> {
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let mut tokens = TokenMap::default();
    let mut entries = Vec::new();
    let mut guarded: Vec<(u64, u32)> = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let hwnd = *hwnd_u64 as isize as HWND;
        let window = match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if window.hwnd == *hwnd_u64 => window,
            _ => return Err(err("refuse: capture target ineligible")),
        };
        if window.identity.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        // Owned-helper binding: sibling exe/class/tag. Generic windows never
        // capture even when otherwise eligible.
        let helper_exe =
            crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
        let snap = crate::test_window::sys::query_owned(
            window.hwnd,
            &helper_exe,
            &me.user_sid,
            me.session_id,
        )
        .map_err(|_| err("refuse: capture target ineligible"))?;
        if snap.tag != window.identity.tag || snap.tag.is_empty() {
            return Err(err("refuse: capture target ineligible"));
        }
        if classify(&window.facts).is_err() {
            return Err(err("refuse: capture target ineligible"));
        }
        guarded.push((window.hwnd, window.identity.pid));
        entries.push(serde_json::json!({
            "hwnd": window.hwnd,
            "pid": window.identity.pid,
            "process_creation": window.identity.process_creation,
            "exe_path": window.identity.exe_path,
            "user_sid": window.identity.user_sid,
            "session_id": window.identity.session_id,
            "tag": window.identity.tag,
            "outer": [window.outer.x, window.outer.y, window.outer.w, window.outer.h],
            "visible": [window.visible.x, window.visible.y, window.visible.w, window.visible.h],
        }));
    }
    if options.hwnds.is_empty() {
        return Err(err("refuse: capture requires at least one --hwnd"));
    }
    // Fresh per-HWND pid guard immediately before the write: a recycled
    // HWND between observation and receipt refuses the whole capture.
    for (hwnd_u64, pid) in &guarded {
        let hwnd = *hwnd_u64 as isize as HWND;
        let mut current: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut current);
        }
        if current == 0 || current != *pid {
            return Err(err("refuse: capture target identity changed"));
        }
    }
    let receipt = serde_json::json!({ "windows": entries }).to_string();
    if options.out.exists() {
        return Err(err("refuse: receipt preexists"));
    }
    std::fs::write(&options.out, &receipt)
        .map_err(|e| err(format!("error: receipt write: {e}")))?;
    Ok(serde_json::json!({"captured": entries.len()}).to_string())
}

/// `inventory` command: read-only top-level window list for selecting capture
/// targets. Reports HWND, pid, executable basename, class, and style bits;
/// never reads titles. Stdout only; nothing is written or tiled.
pub fn cmd_inventory() -> Result<String> {
    ensure_pm_v2()?;
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut entries = Vec::new();
    for raw in hwnds {
        let hwnd = raw as HWND;
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            continue;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .map(|ident| {
                ident
                    .exe_path
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("unknown")
                    .to_owned()
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        entries.push(serde_json::json!({
            "hwnd": raw as u64,
            "pid": pid,
            "exe": exe,
            "class": class_of(hwnd),
            "style": format!("{style:#x}"),
            "exstyle": format!("{exstyle:#x}"),
        }));
    }
    Ok(serde_json::json!({ "windows": entries }).to_string())
}

unsafe extern "system" fn child_proc(hwnd: HWND, state: LPARAM) -> i32 {
    let out = unsafe { &mut *(state as *mut Vec<isize>) };
    out.push(hwnd as isize);
    1
}

/// Enumerate direct child windows of one parent. `None` on API failure.
fn enumerate_child_windows(parent: HWND) -> Option<Vec<isize>> {
    let mut out: Vec<isize> = Vec::new();
    let ok = unsafe {
        EnumChildWindows(
            parent,
            Some(child_proc),
            &mut out as *mut Vec<isize> as LPARAM,
        )
    };
    if ok == 0 { None } else { Some(out) }
}

/// Live executable paths of a top-level window's hosted children: direct
/// children owned by a different process than the top-level window itself.
/// Unreadable children contribute nothing (never a match). Scope
/// classification only: no effects touch child windows. Empty on enumeration
/// failure, which fails closed for listed hosts at the caller's gate.
fn hosted_child_exes(parent: HWND, top_pid: u32) -> Vec<String> {
    let mut out = Vec::new();
    let Some(children) = enumerate_child_windows(parent) else {
        return out;
    };
    for raw in children {
        let hwnd = raw as HWND;
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 || pid == top_pid {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .filter(|ident| ident.pid == pid)
            .map(|ident| ident.exe_path);
        if let Some(exe) = exe {
            out.push(exe);
        }
    }
    out
}

/// Fresh hosted-child gate for one top-level window: listed hosts pass only
/// while a live hosted child matches. Short-circuits before enumerating when
/// the top-level executable names no pair. Read-only; the caller selects the
/// refusal outcome.
fn hosted_gate_allows(
    top_exe: &str,
    hwnd_u64: u64,
    top_pid: u32,
    pairs: &[ScopeHostChild],
) -> bool {
    if pairs.is_empty() {
        return true;
    }
    if !pairs
        .iter()
        .any(|pair| scope_exe_basename(&pair.host) == scope_exe_basename(top_exe))
    {
        return true;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    hosted_child_allows(top_exe, &hosted_child_exes(hwnd, top_pid), pairs)
}

fn exe_basename(exe_path: &str) -> String {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

/// `children` command: read-only child-window report for explicitly listed
/// top-level targets. Each child carries HWND, pid, executable basename,
/// class, and full process identity when readable; never titles. Used to
/// bind hosted app windows (e.g. Calculator under an ApplicationFrameHost
/// root) to their owning process before any capture or mutation.
pub fn cmd_children(options: &ChildrenOptions) -> Result<String> {
    ensure_pm_v2()?;
    let mut windows = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let parent = *hwnd_u64 as isize as HWND;
        let Some(raw_children) = enumerate_child_windows(parent) else {
            return Err(err("error: child enumeration failed"));
        };
        let mut children = Vec::new();
        for raw in raw_children {
            let hwnd = raw as HWND;
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
            let identity = (pid != 0)
                .then(|| HeldProcess::open(pid).ok())
                .flatten()
                .and_then(|held| held.identity().ok())
                .filter(|ident| ident.pid == pid);
            let (exe, detail) = match identity {
                Some(ident) => (
                    exe_basename(&ident.exe_path),
                    serde_json::json!({
                        "pid": ident.pid,
                        "process_creation": ident.process_creation,
                        "exe_path": ident.exe_path,
                        "user_sid": ident.user_sid,
                        "session_id": ident.session_id,
                    }),
                ),
                None => ("unknown".to_owned(), serde_json::Value::Null),
            };
            children.push(serde_json::json!({
                "hwnd": raw as u64,
                "pid": pid,
                "exe": exe,
                "class": class_of(hwnd),
                "identity": detail,
            }));
        }
        windows.push(serde_json::json!({ "hwnd": *hwnd_u64, "children": children }));
    }
    Ok(serde_json::json!({ "windows": windows }).to_string())
}

fn rect_array(rect: &Rect) -> [i32; 4] {
    [rect.x, rect.y, rect.w, rect.h]
}

fn medium_caller() -> Result<ProcessIdentity> {
    let me = crate::native::current_identity().map_err(|e| err(format!("error: identity {e}")))?;
    let rid = crate::native::current_integrity_level()
        .map_err(|e| err(format!("error: integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: integrity {rid} is not medium")));
    }
    Ok(me)
}

fn read_allowlist(path: &Path) -> Result<Vec<AllowEntry>> {
    let text =
        std::fs::read_to_string(path).map_err(|e| err(format!("error: allowlist read: {e}")))?;
    parse_allowlist(&text).map_err(err)
}

/// One inspection entry without frame geometry: full frozen-matchable
/// identity, an accurate skip, and no fabricated rectangles. `outer` and
/// `visible` are null; `dpi` and `minimized` are real queried state.
fn noframe_inspect_report(
    entry: &AllowEntry,
    identity: &ObservedTarget,
    skip: SkipReason,
    minimized: bool,
    hwnd: HWND,
) -> serde_json::Value {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    serde_json::json!({
        "hwnd": entry.hwnd,
        "pid": identity.pid,
        "process_creation": identity.process_creation,
        "exe_path": identity.exe_path,
        "user_sid": identity.user_sid,
        "session_id": identity.session_id,
        "tag": identity.tag,
        "identity_match": true,
        "outer": null,
        "visible": null,
        "dpi": dpi,
        "minimized": minimized,
        "eligible": false,
        "skip": skip.as_str(),
    })
}

/// `inspect` command: read-only fresh-state report for exactly the frozen
/// allowlist. Identity and state resolve before any frame query, so a
/// legitimately hidden (passive) or minimized exact helper keeps full
/// identity match with an accurate skip and no fabricated rectangles; only a
/// genuinely unresolvable or non-matching identity reports
/// `identity-changed`. Raw native identity, fresh outer/visible rectangles
/// from one coherent observe pair when frames exist, per-window DPI, native
/// facts with proof-mode eligibility, and the current foreground HWND. No
/// titles, no writes.
pub fn cmd_inspect(options: &InspectOptions) -> Result<String> {
    use ObserveFailure::Known;
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let allow = read_allowlist(&options.allowlist)?;
    let mut tokens = TokenMap::default();
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    let mut windows = Vec::new();
    for entry in &allow {
        let hwnd = entry.hwnd as isize as HWND;
        // Stateless identity/state first, independent of geometry.
        let ident = window_identity(hwnd, entry.hwnd, &me);
        let matched = ident.as_ref().is_some_and(|i| allow_match(entry, i));
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        let minimized = unsafe { IsIconic(hwnd) } != 0;
        match inspect_stateless_verdict(matched, visible, minimized) {
            StatelessVerdict::Report {
                identity_match,
                // No-frame reports are never eligible; the verdict type
                // guarantees this today.
                eligible: _,
                skip,
            } => {
                match (ident.as_ref(), identity_match) {
                    (Some(identity), true) => windows.push(noframe_inspect_report(
                        entry, identity, skip, minimized, hwnd,
                    )),
                    _ => windows.push(serde_json::json!({
                        "hwnd": entry.hwnd,
                        "identity_match": false,
                        "eligible": false,
                        "skip": skip.as_str(),
                    })),
                }
                continue;
            }
            StatelessVerdict::NeedsGeometry => {}
        }
        // Identity matched a visible, non-minimized window: frame queries and
        // `classify` still decide. A racy frame failure still reports the
        // known identity with its accurate reason, never `identity-changed`.
        match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if allow_match(entry, &window.identity) => {
                let outer = window.outer;
                let dpi = unsafe { GetDpiForWindow(hwnd) };
                let (eligible, skip) = match classify(&window.facts) {
                    Ok(()) => (true, None),
                    Err(reason) => (false, Some(reason.as_str().to_owned())),
                };
                windows.push(serde_json::json!({
                    "hwnd": window.hwnd,
                    "pid": window.identity.pid,
                    "process_creation": window.identity.process_creation,
                    "exe_path": window.identity.exe_path,
                    "user_sid": window.identity.user_sid,
                    "session_id": window.identity.session_id,
                    "tag": window.identity.tag,
                    "identity_match": true,
                    "outer": rect_array(&outer),
                    "visible": rect_array(&window.visible),
                    "dpi": dpi,
                    "facts": {
                        "minimized": window.facts.minimized,
                        "maximized": window.facts.maximized,
                        "cloaked": window.facts.cloaked,
                        "elevated": window.facts.elevated,
                        "shell": window.facts.shell,
                        "tool_window": window.facts.tool_window,
                        "owned": window.facts.owned,
                        "captionless_fullscreen": window.facts.captionless_fullscreen,
                        "no_activate": window.facts.no_activate,
                        "dialog": window.facts.dialog,
                    },
                    "eligible": eligible,
                    "skip": skip,
                }));
            }
            Err(Known(known, reason)) if allow_match(entry, &known.identity) => {
                windows.push(noframe_inspect_report(
                    entry,
                    &known.identity,
                    reason,
                    reason == SkipReason::Minimized,
                    hwnd,
                ));
            }
            _ => windows.push(serde_json::json!({
                "hwnd": entry.hwnd,
                "identity_match": false,
                "eligible": false,
                "skip": SkipReason::IdentityChanged.as_str(),
            })),
        }
    }
    Ok(serde_json::json!({ "windows": windows, "foreground": foreground }).to_string())
}
/// Read-only z-order relation for `border-inspect`: the owned overlay versus
/// the current foreground window in `EnumWindows` top-to-bottom order.
/// `(above, adjacent_below)`. A background owner cannot place its surface
/// above the foreground window, so adjacent-below (directly beneath the
/// target, ring uncovered) is the achievable correct placement. `(None, None)`
/// when either endpoint is gone. No writes.
fn overlay_z_relation(overlay_u64: u64) -> (Option<bool>, Option<bool>) {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if overlay_u64 == 0 || foreground == 0 || overlay_u64 == foreground {
        return (None, None);
    }
    if unsafe { IsWindow(foreground as isize as HWND) } == 0
        || unsafe { IsWindow(overlay_u64 as isize as HWND) } == 0
    {
        return (None, None);
    }
    let order = {
        unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
            let out = unsafe { &mut *(state as *mut Vec<u64>) };
            out.push(hwnd as usize as u64);
            1
        }
        let mut out: Vec<u64> = Vec::new();
        let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<u64> as LPARAM) };
        if ok == 0 {
            return (None, None);
        }
        out
    };
    let mut upper_idx: Option<usize> = None;
    let mut lower_idx: Option<usize> = None;
    for (i, hwnd) in order.iter().enumerate() {
        if *hwnd == overlay_u64 && upper_idx.is_none() {
            upper_idx = Some(i);
        }
        if *hwnd == foreground && lower_idx.is_none() {
            lower_idx = Some(i);
        }
        if upper_idx.is_some() && lower_idx.is_some() {
            break;
        }
    }
    match (upper_idx, lower_idx) {
        (Some(u), Some(l)) if u < l => (Some(true), Some(false)),
        (Some(u), Some(l)) if u == l + 1 => (Some(false), Some(true)),
        (Some(_), Some(_)) => (Some(false), Some(false)),
        _ => (None, None),
    }
}
/// `border-inspect` command: read-only report of the running owner's
/// process-owned overlay window (geometry/visibility only). Binds the exact
/// ledger owner (creation/pid/exe/sid/session) and reports only that owner's
/// overlay class windows: `present` is false when the owner runs with
/// `--no-active-border` or has no eligible target. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `active-border`
/// log events instead.
pub fn cmd_border_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::OVERLAY_CLASS)
}

/// `underlay-inspect` command: read-only report of the running owner's
/// process-owned group-underlay fill (geometry/visibility only), mirroring
/// `border-inspect`. `present` means a carrier HWND was enumerated for this
/// owner (hidden HWNDs persist: `hide` keeps the window alive for the next
/// group, so `present` stays true with `visible=false` after the first show).
/// It is false when the owner runs with `--no-group-underlay` or before the
/// first show. No titles, no content, no screen capture; the owned-pixel
/// checksum rides the owner's `group-underlay` log events instead.
pub fn cmd_underlay_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::UNDERLAY_CLASS)
}

/// `preview-inspect` command: read-only report of the running owner's
/// process-owned drop-preview fill (geometry/visibility only), mirroring
/// `border-inspect`/`underlay-inspect`. `present` means a carrier HWND was
/// enumerated for this owner (hidden HWNDs persist after the first show).
/// It is false before the first preview sample. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `drag-preview`
/// log events instead.
pub fn cmd_preview_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::PREVIEW_CLASS)
}

/// Shared read-only carrier inspection for one owned overlay class.
fn inspect_carrier(class: &str) -> Result<String> {
    use windows_sys::Win32::Foundation::RECT;
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut overlays = Vec::new();
    for raw in hwnds {
        let hwnd = raw as HWND;
        if class_of(hwnd) != class {
            continue;
        }
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 || pid != record.owner.pid {
            continue;
        }
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            continue;
        }
        let Some(rect) = rect_from_win(rect) else {
            continue;
        };
        let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        let (above_fg, adjacent_fg) = overlay_z_relation(raw as usize as u64);
        overlays.push(serde_json::json!({
            "hwnd": raw as u64,
            "visible": unsafe { IsWindowVisible(hwnd) } != 0,
            "rect": rect_array(&rect),
            "dpi": unsafe { GetDpiForWindow(hwnd) },
            "exstyle": format!("{exstyle:08x}"),
            "topmost": exstyle & WS_EX_TOPMOST != 0,
            "above_foreground": above_fg,
            "adjacent_below_foreground": adjacent_fg,
        }));
    }
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    Ok(serde_json::json!({
        "present": !overlays.is_empty(),
        "overlays": overlays,
        "foreground": foreground,
    })
    .to_string())
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

mod shell;

mod monitors;

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

mod cli_resize;

mod cli_workspace;

mod workspace_navigation;

mod workspace_send_tick;

mod migration;

mod migration_run;

mod windrag;

mod pending_drop;

mod gesture_tick;

mod settings;

mod run_loop;

#[cfg(test)]
#[path = "tests_scope_tests.rs"]
mod tiling_sys_scope_tests;

#[cfg(test)]
#[path = "tests_scope_send_tests.rs"]
mod tiling_sys_scope_send_tests;

#[cfg(test)]
#[path = "tests_scope_hotplug_tests.rs"]
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
