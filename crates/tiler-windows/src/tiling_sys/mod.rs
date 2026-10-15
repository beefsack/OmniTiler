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
use self::group_underlay::{
    chord_held_now, classify_move_size_start, hide_preview, hide_underlay, live_outer_rect,
    refresh_group_underlay,
};
use self::hints::{HintCx, HintOutcome, HintStats};
#[cfg(test)]
use self::hints::{clear_test_hints, inject_test_hints};
use self::input::{
    InputBridge, log_input_faults, offer_input_control, offer_input_snapshot, poll_input_ack,
};
use self::keyboard_resize::dispatch_resize_intent;
use self::keyboard_tick::keyboard_tick;
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
use self::owner_work::OwnerWorkGuard;
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
use self::selection::remember_select_departure;
use self::selection::workspace_do_select;
use self::send_follow::workspace_send_follow;
use self::send_native::workspace_do_send_native;
use self::send_preflight::SendPreflight;
use self::send_preflight::SendTarget;
use self::send_stay::ensure_trailing_spare;
use self::send_stay::workspace_send_stay;
use self::sends::workspace_do_send;
use self::shell::{
    DIALOG_CLASS, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, exe_file_name, is_border_shell_popup,
    is_shell_class, rect_from_win,
};
use self::trace::{log_engine_placement_trace, log_json_at, refresh_border_timed};
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

/// Bounded pending gesture drop for the bridged finish path. Owns every
/// input the deferred settle needs (END-time intent, START snapshots,
/// membership routing, carried hover prior, hide pair, deadline) so the
/// service revalidates against fresh observations instead of trusting
/// pre-wait facts. No callbacks, no Engine, no framework: one struct, one
/// service function on the owner loop.
struct PendingDrop {
    hwnd: u64,
    producer: String,
    intent: GestureIntent,
    mover_token: String,
    start_key: Option<crate::workspace::WindowKey>,
    start_tag: Option<Option<String>>,
    member_key: crate::workspace::WindowKey,
    output: String,
    workspace: String,
    drop_prior: Option<tiler_core::session::DragHoverPrior>,
    /// Pointer-resolved cross-output destination plus its END-time fence
    /// snapshot (`None` for same-output finishes, which keep the legacy
    /// source-domain service path).
    dest: Option<PendingCrossDest>,
    /// PRE-gesture allocation of the mover for the source-capture row
    /// substitution (`gesture_before` read at queue time; the map entry is
    /// dropped with the gesture before the ack arrives).
    source_rect: Option<Rect>,
    hide_gen: u64,
    hide_safety: u64,
    deadline: Instant,
}

/// Pointer-resolved cross-output finish: the destination output plus its
/// current workspace at END and the fence snapshot tails re-verify before
/// any Engine or project mutation.
struct PendingCrossDest {
    output: String,
    workspace: String,
    fence: crate::workspace_owner::DragDropFenceSnapshot,
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

/// Test-needed exact-owner keyboard-resize poll for the normal `tile` loop
/// only (tentative, pending user review). Consume-once, exact-owner, and
/// proof refusal ride the shared `poll_workspace_cli_request` prefix shape.
/// The live foreground resolves through the same `snap_origins` map the hook
/// binds chords against (never a carried HWND, never a retarget), and the
/// built intent dispatches through the real `keyboard_tick` resize arm, so
/// every production fence (takeover, suspend/elevated, origin, lifetime,
/// proof, scope, float/sticky, workspace, overlay, busy) and the exact
/// repeat tracker apply unchanged. Logs one `resize` line with edge `cli`
/// for the queued request; the dispatch itself logs the production `resize`
/// line with the scheduled `press_index`.
fn poll_resize_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use tiler_core::directional::Direction;
    let path = dir.join(RESIZE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    let log_path = state.log_path.clone();
    // Best-effort labels for the pre-dispatch logs below: the actual parsed
    // direction/mode when the body parses, `unknown` otherwise. Never fake
    // precision for an unparseable payload.
    let parsed = parse_resize_request(text.trim()).ok();
    let direction_hint = parsed
        .as_ref()
        .map(|request| request.direction.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    let mode_hint = parsed
        .as_ref()
        .map(|request| request.mode.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused-proof",
            }),
        );
        return;
    }
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let Some(request) = parsed else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "resize",
                "tick": tick,
                "mode": mode_hint,
                "direction": direction_hint,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "refused",
            }),
        );
        return;
    };
    let cli_line = |tick: u64, outcome: &'static str| {
        serde_json::json!({
            "event": "resize",
            "tick": tick,
            "mode": request.mode,
            "direction": request.direction,
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "refused"));
        return;
    }
    // Production per-intent fence first, before any observation: a suspended
    // session or an elevated foreground settles here with no side effects.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, outcome));
        return;
    }
    // Direction/mode typed decode onto the classifier vocabulary
    // (`parse_resize_request` already bounded the literals; this match is
    // the exhaustive mapping, never a default).
    let direction = match request.direction.as_str() {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    let mode = match request.mode.as_str() {
        "outwards" => crate::snapkey::ResizeMode::Outwards,
        "inwards" => crate::snapkey::ResizeMode::Inwards,
        _ => {
            state.tick += 1;
            let tick = state.tick;
            log_json_at(&log_path, cli_line(tick, "refused"));
            return;
        }
    };
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(&log_path, cli_line(tick, "unmanaged"));
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = cli_line(tick, "dispatched");
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&log_path, queued);
    keyboard_tick(
        state,
        me,
        fulls,
        areas,
        vec![QueuedSnapEvent::Resize(
            crate::snapkey::QueuedResizeIntent {
                mode,
                direction,
                edge: crate::snapkey::SnapEdge::Down,
                origin: Some(origin),
                consumed: true,
                announce: true,
                tick: Instant::now(),
            },
        )],
        None,
    );
}

/// Exact-owner out-of-hook fullscreen toggle for the normal `tile` loop only
/// (item 9 test-needed route; the hook still filters injected Win+F11).
/// Reuses the production Win+F11 authority verbatim: the `toggle_gate_outcome`
/// suspend/elevated fence (owned-fullscreen exemption preserved), a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), the `member_matches` origin binding, the `revalidate_target`
/// focus/identity/scope/lifetime/proof gates with the focus-relaxed overlay
/// allowance, and the `fullscreen_toggle_decision` direction (including the
/// app-owned R-MAX-05 refusal) plus `enter_fullscreen`/`exit_fullscreen_owned`
/// with the same post-restore reconcile. Consume-once, exact-owner, and proof
/// refusal ride the shared `poll_workspace_cli_request` prefix; this runs only
/// after those pass. Logs `fullscreen-toggle` with edge `cli`.
fn dispatch_fullscreen_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        log_json_at(&state.log_path, settle(outcome));
        return;
    }
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(&state.log_path, settle("observation-failed"));
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        log_json_at(&state.log_path, settle("unmanaged"));
        return;
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin.token.as_str())
        .map(|(key, _)| key.clone())
    else {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    if !crate::workspace_owner::member_matches(
        &member_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        let mut line = settle("foreground-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    // Fresh pre-effect revalidation through the production target gate,
    // identical to the hook path (retained overlay construction included).
    let owned_expected: Option<ObservedWindow>;
    let expected = if let Some(found) = observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        found
    } else {
        let Some(row) = retained.iter().find(|r| r.key == member_key) else {
            let mut line = settle("deferred");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
            let mut line = settle("unmanaged");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        owned_expected = Some(ObservedWindow {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            outer: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            insets: FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: row.key.hwnd,
                pid: row.key.pid,
                process_creation: row.key.creation.clone(),
                exe_path: stored.exe_path,
                user_sid: stored.user_sid,
                session_id: stored.session_id,
                tag: String::new(),
            },
            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        });
        owned_expected.as_ref().expect("retained expected built")
    };
    let from = origin.token.clone();
    let proof_mode = state.allowlist.is_some();
    let member_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => {
            if reason == "identity-changed" {
                drop_member_state(state, &member_key);
            }
            let mut line = settle(reason);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        }
    };
    let _held = &target.held;
    let now_fullscreen = target.window.facts.captionless_fullscreen;
    let meta = read_fullscreen_meta(member_key.hwnd);
    let (target_label, outcome) = match fullscreen_toggle_decision(now_fullscreen, meta.is_some()) {
        crate::tiling::FullscreenToggle::RefuseAppOwned => {
            ("fullscreen", "fullscreen-refused-app-owned")
        }
        crate::tiling::FullscreenToggle::Enter => {
            let full = output_for_rect(areas, &target.window.visible);
            let full = areas
                .iter()
                .find(|a| a.device == full)
                .map(|area| area.full)
                .or_else(|| areas.first().map(|area| area.full));
            match full {
                Some(full) => ("fullscreen", enter_fullscreen(member_key.hwnd, full)),
                None => ("fullscreen", "unknown-output"),
            }
        }
        crate::tiling::FullscreenToggle::ExitOwned => {
            let meta = meta.expect("decision owns metadata");
            let outcome = exit_fullscreen_owned(member_key.hwnd, &meta);
            if outcome == "restored" && !meta.was_maximized {
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
            ("restored", outcome)
        }
    };
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
            "target": target_label,
            "origin": origin.token,
            "window": from,
        }),
    );
}

/// Exact-owner out-of-hook float toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+G). Reuses the production Win+G authority verbatim
/// through `dispatch_float_intent`: the suspend/elevated fence, a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), origin re-resolution with member/lifetime/proof/scope fences, the
/// overlay and workspace-mode fences, and the Engine-candidate commit with
/// marker persistence. Consume-once, exact-owner, and proof refusal ride the
/// shared `poll_workspace_cli_request` prefix; this runs only after those
/// pass. The outer `dispatched` line carries edge `cli`; the dispatched arm
/// logs its own `float-toggle` outcome lines. No synthetic input accepted.
fn dispatch_float_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "float-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_float_intent(
        state,
        me,
        fulls,
        areas,
        QueuedFloatIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook sticky toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+Shift+G). Reuses the production Win+Shift+G
/// authority verbatim through `dispatch_sticky_intent` with the same fences
/// and origin contract as the float arm above. Logs `sticky-toggle`.
fn dispatch_sticky_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "sticky-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_sticky_intent(
        state,
        me,
        fulls,
        areas,
        QueuedStickyIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook workspace control: one bounded `workspace.request`
/// file consumed once through the existing `workspace_do_select` /
/// `workspace_do_send` resolvers (plus the pure history resolvers for the
/// previous/relative forms), through the existing project-owned fullscreen
/// toggle below for `fullscreen`, or through the existing hook float/sticky
/// dispatch arms below for `float`/`sticky`. Normal `tile` only (proof
/// owners refuse without effect); workspace ops gate fullscreen and elevated
/// foreground like the hook path while the fullscreen/float/sticky ops route
/// through the production `toggle_gate_outcome` (owned-fullscreen exemption
/// preserved); the keyboard takeover switch never gates this (out-of-hook
/// dogfood/recovery). The request is deleted before dispatch so there is no
/// replay; a malformed or mismatched body is consumed the same way with a
/// `refused` outcome. Sends carry no chord origin: the mover is the live
/// foreground managed window at dispatch, resolved through the same
/// `snap_origins` map the hook binds chords against, then the exact same
/// `workspace_do_send` focus/identity/scope/owner gates with the request's
/// explicit follow/stay intent. Fullscreen/float/sticky carry no HWND either:
/// they toggle the live foreground managed window through the same
/// `revalidate_target` + toggle authority as their hook chords (including
/// the app-owned R-MAX-05 refusal for fullscreen). Production log
/// carries op/index/edge/outcome only (no HWNDs, tokens, or identity bytes);
/// the client correlation is opaque and never logged.
fn poll_workspace_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    // Honest op label for the pre-parse refusal: the parsed op when the body
    // is well-formed JSON carrying a known action, else `select`.
    let op_hint = |body: &str| -> String {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("action").and_then(|a| a.as_str().map(str::to_owned)))
            .filter(|a| {
                a == "select"
                    || a == "send"
                    || a == "stay"
                    || a == "previous"
                    || a == "fullscreen"
                    || a == "float"
                    || a == "sticky"
                    || a == "relative"
                    || a == "send-relative"
                    || a == "stay-relative"
            })
            .unwrap_or_else(|| "select".to_owned())
    };
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let op = op_hint(text.trim());
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, &op, 0, "cli", "refused-proof"),
        );
        return;
    }
    let parsed = parse_workspace_request(text.trim());
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let request = match parsed {
        Ok(request) => request,
        Err(_) => {
            let op = op_hint(text.trim());
            state.tick += 1;
            let tick = state.tick;
            log_json_at(
                &state.log_path,
                workspace_log(state, tick, &op, 0, "cli", "refused"),
            );
            return;
        }
    };
    let op = request.action.as_str();
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "refused"),
        );
        return;
    }
    if request.action == WorkspaceAction::Fullscreen {
        dispatch_fullscreen_cli_toggle(state, me, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Float {
        dispatch_float_cli_toggle(state, me, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Sticky {
        dispatch_sticky_cli_toggle(state, me, fulls, areas);
        return;
    }
    if suspend_read(state, me, fulls).veto.block {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "suspended"),
        );
        return;
    }
    if foreground_elevated(me) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "elevated-foreground"),
        );
        return;
    }
    let Some(output) = chord_output(state, areas) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "unknown-output"),
        );
        return;
    };
    state.workspaces.ensure_output(&output);
    state.tick += 1;
    let tick = state.tick;
    // Dispatch beginning for the CLI path: the clock starts before the
    // initial observation. No queued chord here, so the queue wait stays
    // zero by construction.
    let start = Instant::now();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "observation-failed"),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    if request.action.is_send() {
        // Boundary-send testing affordance: the normal loop deliberately
        // rejects injected keys, so the mover is the live foreground managed
        // window at dispatch (same `snap_origins` map the hook binds chords
        // against), never a carried HWND. The request's explicit follow/stay
        // intent threads the exact same `workspace_do_send` gates; old
        // `--send` keeps following. Every other gate stays inside the
        // existing `workspace_do_send`.
        let (target, follow, dispatch_op): (SendTarget, bool, &'static str) = match request.action {
            WorkspaceAction::Send => (SendTarget::Numbered(request.index), true, "send"),
            WorkspaceAction::Stay => (SendTarget::Numbered(request.index), false, "stay"),
            WorkspaceAction::RelativeSend => match request.direction {
                Some(direction) => {
                    let target = SendTarget::Relative(direction.delta());
                    (target, true, target.log_op())
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::RelativeStay => match request.direction {
                Some(direction) => {
                    let delta = direction.delta();
                    (
                        SendTarget::Relative(delta),
                        false,
                        if delta < 0 { "stay-prev" } else { "stay-next" },
                    )
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::Select
            | WorkspaceAction::Previous
            | WorkspaceAction::Fullscreen
            | WorkspaceAction::Float
            | WorkspaceAction::Sticky
            | WorkspaceAction::RelativeHistory => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, op, request.index, "cli", "refused"),
                );
                return;
            }
        };
        let dispatch_index = match request.action {
            WorkspaceAction::Send | WorkspaceAction::Stay => request.index,
            _ => 0,
        };
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = state.snap_origins.get(&foreground_hwnd).cloned();
        match origin.filter(|o| !o.token.is_empty()) {
            Some(o) => {
                let effect = workspace_do_send(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &retained,
                    &output,
                    target,
                    follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &state.log_path,
                    workspace_log(
                        state,
                        tick,
                        dispatch_op,
                        dispatch_index,
                        "cli",
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &state.log_path.clone(),
                    &ActionLine {
                        ctx: &ctx,
                        op: dispatch_op,
                        index: dispatch_index,
                        edge: "cli",
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: effect.source_plan_ms,
                            target_plan_ms: effect.target_plan_ms,
                            source_geometry_ms: effect.source_geometry_ms,
                            target_geometry_ms: effect.target_geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: effect.source.as_ref(),
                        target: effect.target.as_ref(),
                    },
                );
            }
            _ => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, dispatch_op, dispatch_index, "cli", "unmanaged"),
                );
            }
        }
        return;
    }
    // Resolve without preactivating (same as the hook select path):
    // resolvers never touch ACTIVE; only the transition activates. History
    // forms resolve purely like the hook history path. Scoped resolvers keep
    // the local path equivalent and share trailing creation across outputs.
    let (target, none_outcome, dispatch_op, dispatch_index): (
        Option<String>,
        &'static str,
        &'static str,
        u8,
    ) = match request.action {
        WorkspaceAction::Select => {
            if request.index == 0 {
                let sticky: BTreeSet<crate::workspace::WindowKey> =
                    state.sticky.keys().cloned().collect();
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                match owner.resolve_trailing_select(spaces, &output, &sticky) {
                    crate::workspace_scope_owner::TrailingResolve::Reuse(id) => {
                        (Some(id), "unknown-output", "select", request.index)
                    }
                    crate::workspace_scope_owner::TrailingResolve::Unknown => {
                        (None, "unknown-output", "select", request.index)
                    }
                }
            } else {
                let target =
                    state
                        .scope_owner
                        .resolve_numbered(&state.workspaces, &output, request.index);
                (target, "unknown-target", "select", request.index)
            }
        }
        WorkspaceAction::Previous => {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            (
                owner.resolve_previous(spaces, &output),
                "unknown-target",
                "previous",
                0,
            )
        }
        WorkspaceAction::RelativeHistory => match request.direction {
            Some(direction) => {
                let delta = direction.delta();
                (
                    state
                        .scope_owner
                        .resolve_relative(&state.workspaces, &output, delta),
                    "unknown-target",
                    if delta < 0 {
                        "relative-prev"
                    } else {
                        "relative-next"
                    },
                    0,
                )
            }
            None => (None, "refused", "relative", 0),
        },
        WorkspaceAction::Send
        | WorkspaceAction::Stay
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => (None, "refused", op, request.index),
    };
    let Some(target) = target else {
        log_json_at(
            &state.log_path,
            workspace_log(
                state,
                tick,
                dispatch_op,
                dispatch_index,
                "cli",
                none_outcome,
            ),
        );
        return;
    };
    // REQ-WS-09 departure memory for the CLI select/previous/relative path:
    // the live foreground resolves through the same `snap_origins` map the
    // hook binds chords against (never a carried HWND).
    let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
    if let Some(source_id) = state.workspaces.active_id(&output)
        && let Some(origin) = state.snap_origins.get(&foreground_hwnd).cloned()
        && !origin.token.is_empty()
        && let Some(token) = remember_select_departure(
            state,
            &origin,
            &output,
            &source_id,
            foreground_hwnd,
            &observed,
            &retained,
        )
    {
        log_json_at(
            &state.log_path.clone(),
            serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
        );
    }
    let effect = workspace_do_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &mut observed,
        &output,
        &target,
        &ctx,
        None,
        false,
        false,
    );
    log_json_at(
        &state.log_path,
        workspace_log(
            state,
            tick,
            dispatch_op,
            dispatch_index,
            "cli",
            effect.outcome,
        ),
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: dispatch_op,
            index: dispatch_index,
            edge: "cli",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
}

/// Drain one bounded batch of workspace digit intents. Select works on empty
/// workspaces and unmanaged foreground; send refuses unmanaged focus. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (the hook is never installed; this is the defensive second
/// fence). Fullscreen and elevated foreground gate both ops.
#[allow(clippy::too_many_arguments)]
fn workspace_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "index": intent.index,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        // Dispatch beginning: the action clock starts here, before the
        // initial observation, so queue-to-effect latency includes the
        // observation itself. `queued_at` stays the cheap callback stamp.
        // Verified managed overlays bypass suspension like a managed maximize.
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        // First-seen maximized windows restore once here: lifetime tags are
        // stamped, and the cleared window converges as eligible next tick.
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        // Stable per-action context: one opaque correlation for the whole
        // select/send/follow lifecycle even when inner geometry ticks
        // increment. `queued_at` is the cheap callback stamp (no
        // logging/syscalls in the callback); `start` is the dispatch
        // beginning on the loop thread, captured before observation.
        let op = intent.op;
        let index = intent.index;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        match op {
            WorkspaceOp::Select => {
                // Resolve without preactivating: `select`/`select_trailing`
                // mutate ACTIVE, which makes `workspace_do_select` see
                // `current == target` and return `already-active` with no
                // native effects. Scoped resolvers resolve (minting shared
                // trailing when needed) without touching ACTIVE; only the
                // transition activates after hide/reveal.
                let target: Option<String> = if index == 0 {
                    let sticky: BTreeSet<crate::workspace::WindowKey> =
                        state.sticky.keys().cloned().collect();
                    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                    match owner.resolve_trailing_select(spaces, &output, &sticky) {
                        crate::workspace_scope_owner::TrailingResolve::Reuse(id) => Some(id),
                        crate::workspace_scope_owner::TrailingResolve::Unknown => None,
                    }
                } else {
                    state
                        .scope_owner
                        .resolve_numbered(&state.workspaces, &output, index)
                };
                match target {
                    Some(id) => {
                        // REQ-WS-09 departure memory: the chord-time origin
                        // still holds live foreground, so the source
                        // remembers it before hiding.
                        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                        if let Some(source_id) = state.workspaces.active_id(&output)
                            && let Some(origin) = intent.origin.as_ref()
                            && let Some(token) = remember_select_departure(
                                state,
                                origin,
                                &output,
                                &source_id,
                                foreground_hwnd,
                                &observed,
                                &retained,
                            )
                        {
                            log_json_at(
                                &state.log_path.clone(),
                                serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                            );
                        }
                        let effect = workspace_do_select(
                            state,
                            me,
                            store,
                            dir,
                            fulls,
                            areas,
                            &mut observed,
                            &output,
                            &id,
                            &ctx,
                            None,
                            false,
                            false,
                        );
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
                                edge: edge.as_str(),
                                outcome: effect.outcome,
                                focus: effect.focus,
                                timings: &ActionTimings {
                                    transition_ms: effect.transition_ms,
                                    observation_ms: effect.observation_ms,
                                    source_plan_ms: 0,
                                    target_plan_ms: effect.plan_ms,
                                    source_geometry_ms: 0,
                                    target_geometry_ms: effect.geometry_ms,
                                    hide_ms: effect.hide_ms,
                                    reveal_ms: effect.reveal_ms,
                                    focus_ms: effect.focus_ms,
                                },
                                source_workspace: &effect.source_workspace,
                                target_workspace: &effect.target_workspace,
                                source: None,
                                target: effect.geometry.as_ref(),
                            },
                        );
                    }
                    None => {
                        let outcome = if index == 0 {
                            "unknown-output"
                        } else {
                            "unknown-target"
                        };
                        log_json_at(
                            &log_path,
                            workspace_log(state, tick, op.as_str(), index, edge.as_str(), outcome),
                        );
                    }
                }
            }
            WorkspaceOp::Send => {
                let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                let origin = intent.origin.clone();
                match origin.filter(|o| o.hwnd == foreground_hwnd) {
                    Some(o) if !o.token.is_empty() => {
                        let effect = workspace_do_send(
                            state,
                            me,
                            store,
                            dir,
                            fulls,
                            areas,
                            &mut observed,
                            &retained,
                            &output,
                            SendTarget::Numbered(index),
                            intent.follow,
                            o.hwnd,
                            &o.token,
                            o.pid,
                            &o.creation,
                            &ctx,
                        );
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
                                edge: edge.as_str(),
                                outcome: effect.outcome,
                                focus: effect.focus,
                                timings: &ActionTimings {
                                    transition_ms: effect.transition_ms,
                                    observation_ms: effect.observation_ms,
                                    source_plan_ms: effect.source_plan_ms,
                                    target_plan_ms: effect.target_plan_ms,
                                    source_geometry_ms: effect.source_geometry_ms,
                                    target_geometry_ms: effect.target_geometry_ms,
                                    hide_ms: effect.hide_ms,
                                    reveal_ms: effect.reveal_ms,
                                    focus_ms: effect.focus_ms,
                                },
                                source_workspace: &effect.source_workspace,
                                target_workspace: &effect.target_workspace,
                                source: effect.source.as_ref(),
                                target: effect.target.as_ref(),
                            },
                        );
                    }
                    _ => {
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                "unmanaged",
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Drain one bounded batch of item 2 relative workspace-send intents.
/// Previous/next steps resolve once from the mover's source workspace through
/// the item 1 scoped ring (wrapping through the trailing empty and ordinals
/// beyond 9, never MRU, never creating); filling the trailing empty invokes
/// ordinary lifecycle for the next spare after transfer. Follow/stay ride the
/// explicit intent exactly like numbered sends (`--no-keyboard-snap-takeover`
/// off, fullscreen/elevated gates, unmanaged-focus refusal all apply). Only
/// verified follow selects record history; stay never does.
#[allow(clippy::too_many_arguments)]
fn workspace_relative_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceSendIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-send",
                        "tick": state.tick,
                        "delta": intent.delta,
                        "follow": intent.follow,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "tick": tick,
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let target = SendTarget::Relative(intent.delta);
        let op = target.log_op();
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = intent.origin.clone();
        match origin.filter(|o| o.hwnd == foreground_hwnd) {
            Some(o) if !o.token.is_empty() => {
                let effect = workspace_do_send(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &retained,
                    &output,
                    target,
                    intent.follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op,
                        index: target.log_index(),
                        edge: edge.as_str(),
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: effect.source_plan_ms,
                            target_plan_ms: effect.target_plan_ms,
                            source_geometry_ms: effect.source_geometry_ms,
                            target_geometry_ms: effect.target_geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: effect.source.as_ref(),
                        target: effect.target.as_ref(),
                    },
                );
            }
            _ => {
                log_json_at(
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        "unmanaged",
                    ),
                );
            }
        }
    }
}

/// Drain one bounded batch of item 5 explicit output-send intents.
/// Each intent resolves once from the mover's current frame through the
/// shared FULL-rectangle window-centre/span/left-top selector into the
/// destination output's CURRENT workspace (no candidate is a no-op,
/// unreadable topology refuses, outputs never wrap), then transfers through
/// the commit-last [`run_output_send`] route with command follow/stay exactly
/// like numbered sends (`--no-keyboard-snap-takeover` off, fullscreen/
/// elevated gates, unmanaged-focus refusal all apply). Follow focuses the
/// verified mover on the target output; stay keeps the source view with its
/// MRU (null takes no setter) while the target current view keeps the mover
/// visible.
#[allow(clippy::too_many_arguments)]
fn workspace_output_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceOutputIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-output-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-output-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-output",
                        "tick": state.tick,
                        "direction": direction_name(intent.direction),
                        "follow": intent.follow,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let direction = direction_name(intent.direction);
        let edge = intent.edge.as_str();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = intent.origin.clone();
        let Some(o) = origin.filter(|o| o.hwnd == foreground_hwnd) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unmanaged",
                }),
            );
            continue;
        };
        if o.token.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unmanaged",
                }),
            );
            continue;
        }
        let outcome = workspace_output_send_one(
            state,
            me,
            fulls,
            areas,
            &mut observed,
            &retained,
            &output,
            &intent,
            &o,
            tick,
        );
        // `run_output_send` logs the full action line; narrow pre-dispatch
        // refusals log here.
        let _ = outcome;
    }
}

/// Dispatch one foreground-verified explicit output-send intent: exact origin
/// binding, scope/lifetime/proof gates, pinned source plus complete source
/// observation, one selector resolution, then the commit-last transfer.
#[allow(clippy::too_many_arguments)]
fn workspace_output_send_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    intent: &QueuedWorkspaceOutputIntent,
    origin: &crate::snapkey::SnapOrigin,
    tick: u64,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_refuse = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-output",
                "tick": tick,
                "direction": direction_name(intent.direction),
                "follow": intent.follow,
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
            }),
        );
    };
    let mover_key = state
        .member_tokens
        .iter()
        .find(|(_, t)| t.as_str() == origin.token)
        .map(|(k, _)| k.clone());
    let Some(mover_key) = mover_key else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    // Full chord-time binding: token plus HWND, PID, and process creation.
    // A recycled HWND or a retargeted token never dispatches.
    if !crate::workspace_owner::member_matches(
        &mover_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        log_refuse("foreground-changed");
        return "foreground-changed";
    }
    let Some(home) = state.workspaces.member_loc(&mover_key).cloned() else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    if home.output != output || state.workspaces.is_hidden(&mover_key) {
        log_refuse("unmanaged");
        return "unmanaged";
    }
    // Exact scope and visible-lifetime gates before any Engine or project
    // mutation; tail-time fences re-verify live inside `run_output_send`.
    // Sticky/float subject eligibility rides the same run entry so the
    // headless seams cover it without a foreground window.
    if let Some(stored) = state.member_identity.get(&mover_key) {
        if !scope_allows(&state.scope, &stored.exe_path) {
            log_refuse("scope-excluded");
            return "scope-excluded";
        }
        if !hosted_gate_allows(
            &stored.exe_path,
            origin.hwnd,
            stored.pid,
            &state.scope_hosts,
        ) {
            log_refuse("scope-excluded");
            return "scope-excluded";
        }
    }
    let live_tag = live_member_tag(origin.hwnd);
    let lifetime_ok = state.member_tags.get(&mover_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &mover_key);
        log_refuse("identity-changed");
        return "identity-changed";
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        log_refuse("workspace-disabled");
        return "workspace-disabled";
    }
    // Complete pinned source observation (hidden snapshots included) so the
    // planned mutation reuses topology instead of remove/reseed. One shared
    // hint-query budget across both assemblies.
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        &home.output,
        &home.workspace,
        observed,
        retained,
        "output-send",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    let Some(mover_row) = source_rows.iter().find(|r| r.token == origin.token) else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    let mover_rect = mover_row.rect;
    // Resolve once through the shared FULL-rectangle selector; the pinned
    // target never re-resolves off the relocated mover below.
    match resolve_output_send_target(state, areas, &home.output, intent.direction, &mover_rect) {
        Ok(None) => {
            log_refuse("no-op");
            "no-op"
        }
        Err(outcome) => {
            log_refuse(outcome);
            outcome
        }
        Ok(Some((target_output, target_workspace))) => {
            state.workspaces.ensure_output(&target_output);
            // Overlay/subject eligibility rides the run entry (which sees
            // both the observed and retained mover); the pinned source rows
            // and this resolution stay the only pre-transfer inputs here.
            run_output_send(
                state,
                me,
                fulls,
                areas,
                observed,
                retained,
                &mut hint_cx,
                &home,
                &mover_key,
                &origin.token,
                &source_rows,
                &target_output,
                &target_workspace,
                intent,
                origin,
                tick,
                &correlation,
                correlation.as_str(),
            )
        }
    }
}

/// Drain one bounded batch of item 14 whole-active-workspace migration intents
/// (R-WS-12 D1-D9/G37). Each intent migrates the ACTIVE workspace of the chord
/// output only (follow-only, unbound by default): resolve the adjacent output
/// once through the shared largest-shared-edge FULL-rectangle selector (no
/// candidate is a no-op, unreadable topology refuses, outputs never wrap),
/// then transfer through the commit-last [`run_workspace_migrate`] route with
/// the live G-37 source-refill mode. D4 retains per-output tiling modes:
/// untiled source and untiled target prior are allowed (independent
/// project-owned views, never a both-tiled gate); mode drift fences below.
/// `--no-keyboard-snap-takeover` off, fullscreen/elevated gates,
/// observation failure and unknown-output refusals mirror the output-send
/// tick.
#[allow(clippy::too_many_arguments)]
fn workspace_migrate_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceMigrateIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-migrate",
                        "tick": state.tick,
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let direction = direction_name(intent.direction);
        let edge = intent.edge.as_str();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        // `run_workspace_migrate` logs the full action line; narrow
        // pre-dispatch refusals log here.
        let _ = workspace_migrate_one(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &mut observed,
            &retained,
            &output,
            &intent,
            tick,
        );
    }
}

/// Dispatch one observed migration intent on its active output: pin the source
/// active workspace, resolve the adjacent target once through the shared
/// largest-edge selector, then run the commit-last transfer. The pinned
/// source/target pair never re-resolves below.
#[allow(clippy::too_many_arguments)]
fn workspace_migrate_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_refuse = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-migrate",
                "tick": tick,
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
    };
    // Item 12 stage B: shared whole-workspace migration refuses per the
    // established contract before any mutation.
    if let Some(outcome) = scope_sends::migration_scope_refusal(state) {
        log_refuse(outcome);
        return outcome;
    }
    let Some(source_workspace) = state.workspaces.active_id(output) else {
        log_refuse("unknown-output");
        return "unknown-output";
    };
    let outputs = directional_outputs_from_areas(areas);
    let selected =
        match crate::workspace_owner::select_migration_target(intent.direction, output, &outputs) {
            Ok(selected) => selected,
            Err(_) => {
                log_refuse("unreadable-topology");
                return "unreadable-topology";
            }
        };
    let Some(target_output) = selected else {
        log_refuse("no-op");
        return "no-op";
    };
    if target_output == output {
        log_refuse("no-op");
        return "no-op";
    }
    state.workspaces.ensure_output(&target_output);
    // The destination's DIFFERENT current workspace stays adapter-owned: it
    // is the pinned prior for insertion (D4: the moved id inserts
    // immediately after it) and for fences, never a carried domain. The
    // target migration domain carries the SAME workspace id on the target
    // output with empty target windows (the Engine refuses carried target
    // residue).
    let Some(target_prior) = state.workspaces.active_id(&target_output) else {
        log_refuse("unknown-output");
        return "unknown-output";
    };
    // D4 mode retention: independent project-owned output views carry
    // their own tiling modes, so untiled source and untiled target prior are
    // both allowed (no both-tiled gate). Mode drift still fences below via
    // the snapshot/live tiled pair; an untiled source takes move-only native
    // transfer with no tiled geometry and no Engine dispatch.
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        output,
        &source_workspace,
        observed,
        retained,
        "workspace-migrate",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    let Some(migrate_rows) = migrate_rows_for(
        state,
        output,
        &source_workspace,
        &source_rows,
        observed,
        retained,
        fulls,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    run_workspace_migrate(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        retained,
        &mut hint_cx,
        output,
        &source_workspace,
        &source_rows,
        &migrate_rows,
        &target_output,
        &target_prior,
        intent,
        tick,
        &correlation,
        correlation.as_str(),
    )
}

/// Build one [`crate::workspace_owner::MigrateRow`] per source member with
/// live observations, never fabricated placeholders: `floating` rides the
/// assembled Engine row (minimized/hidden omitted rows fall back to the
/// runtime float intent plus the retained Engine exception, so a carried
/// minimized float is never downgraded to tiled), `sticky` rides the runtime
/// sticky set, `maximized` re-reads live via `IsZoomed`
/// ([`live_overlay_flags`]) for visible members and falls back to the
/// retained row for hidden/retained members, `fullscreen` re-reads live with
/// the same fallback, `fixed_auto` rides the automatic-fixed latch, and
/// `fit_excluded` rides the float exception (the only unexplained-fit
/// exclusion the Engine accepts is carried by a
/// floating/sticky/fullscreen/maximized origin). Minimized/iconic-hidden
/// members omitted from the assembled rows carry their retained normal frame
/// (`retained.rect`, else `member_rects`, else `float_rects`) with no hint so
/// the Engine keeps their slot and the native placement tail can remap them.
/// Returns `None` when any member lacks a token/rect snapshot: the caller
/// defers with retained state.
fn migrate_rows_for(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    fulls: &[Rect],
) -> Option<Vec<crate::workspace_owner::MigrateRow>> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_token: HashMap<&str, &crate::workspace_owner::OwnerRow> = source_rows
        .iter()
        .map(|row| (row.token.as_str(), row))
        .collect();
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed
        .iter()
        .map(|window| (window.hwnd, window))
        .collect();
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let mut rows = Vec::with_capacity(members.len());
    for key in &members {
        let token = state.member_tokens.get(key).cloned()?;
        let sticky = state.sticky.contains_key(key);
        let hidden = state.workspaces.is_hidden(key);
        let omitted = state.minimized_omitted.contains(key);
        let (maximized, fullscreen) = if hidden || omitted {
            // Hidden/minimized members keep no live frame: carry the retained
            // overlay flags so a carried overlay is never downgraded to a
            // plain row (which would drop its slot) nor invented.
            let retained_row = retained.iter().find(|r| r.key == *key);
            match retained_row {
                Some(r) => (r.maximized, r.fullscreen),
                None => {
                    let overlay = state.overlay_now.get(&token);
                    (
                        overlay.is_some_and(|(max, _)| *max),
                        overlay.is_some_and(|(_, full)| *full),
                    )
                }
            }
        } else if by_hwnd.contains_key(&key.hwnd) {
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, fulls);
            let fallback_full = retained
                .iter()
                .find(|r| r.key == *key)
                .map(|r| r.fullscreen)
                .or_else(|| state.overlay_now.get(&token).map(|(_, full)| *full))
                .unwrap_or(false);
            (live_max, live_full_opt.unwrap_or(fallback_full))
        } else {
            let r = retained.iter().find(|r| r.key == *key)?;
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, fulls);
            (live_max, live_full_opt.unwrap_or(r.fullscreen))
        };
        if let Some(row) = by_token.get(token.as_str()) {
            rows.push(crate::workspace_owner::MigrateRow {
                token: token.clone(),
                rect: row.rect,
                hints: row.hints,
                floating: row.floating,
                fullscreen,
                maximized,
                sticky,
                fixed_auto: state.fixed_auto_seen.contains(key),
                fixed_suppress: false,
                fit_excluded: row.floating,
            });
            continue;
        }
        // Minimized/iconic-hidden omission: the assembled rows skip the
        // frameless non-overlay row so convergence can store the
        // `MinimizedSlot`, but migration must still carry the member (else
        // the Engine refuses `PartialObservation` and the native placement
        // tail could never commit). Carry the retained normal frame with no
        // hint; overlay flags above already preserve max/fullscreen.
        let rect = retained
            .iter()
            .find(|r| r.key == *key)
            .and_then(|r| r.rect)
            .or_else(|| state.member_rects.get(&token).copied())
            .or_else(|| state.float_rects.get(&token).copied())?;
        let floating = state.floated.contains(key)
            || state
                .engine
                .session(&domain_key)
                .is_some_and(|session| session.is_exception(&WindowId(token.clone())));
        rows.push(crate::workspace_owner::MigrateRow {
            token: token.clone(),
            rect,
            hints: WindowSizeHints::none(),
            floating,
            fullscreen,
            maximized,
            sticky,
            fixed_auto: state.fixed_auto_seen.contains(key),
            fixed_suppress: false,
            fit_excluded: floating,
        });
    }
    Some(rows)
}

/// Stage the native view publication for one migration (D4/G37): hide the
/// occupied target prior (ledger-authorized, stays listed and hidden) and
/// reveal the source refill plus migrated product-hidden members as the
/// active views permit (preserving minimized/hidden intent, never
/// unminimizing). `candidate_ws` already holds the project reassignment
/// (moved shown on target, refill shown on source); this applies the native
/// hide/show effects fenced per effect and mirrors the hidden flags onto the
/// candidate so publication carries both physical views. Under test the
/// ledger setters are stubbed to project flag moves (native-free); production
/// routes through [`workspace_hide_one`] and `reveal_product_claim_to` with
/// the recovery ledger (never bypassed). Any effect failure reconciles
/// without replay; the caller publishes only after both views verify.
/// Returns `None` when all effects hold, else the refusal outcome.
#[allow(clippy::too_many_arguments)]
fn migration_view_effects(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    candidate_ws: &mut crate::workspace::ManagedWorkspaces,
    source_output: &str,
    refill: Option<&str>,
    target_output: &str,
    target_prior: &str,
    migrating: &BTreeSet<crate::workspace::WindowKey>,
    snapshot: &crate::workspace_owner::MigrateFenceSnapshot,
    tail_areas: &[MonitorArea],
) -> Option<&'static str> {
    // Per-effect fence gate shared by every hide/reveal below: real scope,
    // lifetime, membership plus fresh topology on EACH call (never stubbed,
    // never fixed areas). The project reassignment has not published yet, so
    // membership still homes every migrating member on the source.
    let fences_hold =
        |state: &TileLoop, source_workspace: &str, target_prior: &str| -> Option<&'static str> {
            let scope_ok = migrating.iter().all(|key| {
                state.member_identity.get(key).is_some_and(|stored| {
                    scope_allows(&state.scope, &stored.exe_path)
                        && hosted_gate_allows(
                            &stored.exe_path,
                            key.hwnd,
                            stored.pid,
                            &state.scope_hosts,
                        )
                })
            });
            let lifetime_ok = migrating.iter().all(|key| {
                state.member_tags.get(key).is_some_and(|stored| {
                    crate::workspace_owner::visible_lifetime_ok(
                        stored,
                        live_member_tag(key.hwnd).as_deref(),
                    )
                })
            });
            let membership_ok = migrating.iter().all(|key| {
                state.workspaces.member_loc(key).is_some_and(|fresh| {
                    fresh.output == source_output && fresh.workspace == source_workspace
                })
            });
            let topology_ok = fresh_cross_areas().is_some_and(|fresh| {
                cross_topology_fresh(
                    &crate::workspace_owner::migrate_snapshot_for_drag(snapshot),
                    &fresh,
                )
            });
            let live = crate::workspace_owner::MigrateFenceLive {
                inner_gap: state.inner_gap,
                outer_gap: state.outer_gap,
                source_active: state.workspaces.active_id(source_output),
                target_active: state.workspaces.active_id(target_output),
                source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
                target_tiled: state.workspaces.is_tiled(target_output, target_prior),
                topology_ok,
                scope_ok,
                lifetime_ok,
                membership_ok,
                views_ok: state.workspaces.active_id(source_output).as_deref()
                    == Some(source_workspace)
                    && state.workspaces.active_id(target_output).as_deref() == Some(target_prior),
                owner_ok: state
                    .engine
                    .owner()
                    .is_some_and(|owner| owner.as_str() == snapshot.owner.as_str())
                    && state.engine.generation().is_some_and(|generation| {
                        generation.as_str() == snapshot.generation.as_str()
                    }),
            };
            crate::workspace_owner::migrate_fences_hold(snapshot, &live)
        };
    // Pinned source workspace for the pre-commit view fences (the project
    // reassignment has not published yet, so the source still shows it).
    let source_workspace = snapshot.moved_workspace.as_str();
    let mut newly_hidden: Vec<crate::workspace::WindowKey> = Vec::new();
    // Hide the occupied target prior: visible members take a ledger hide and
    // stay listed but hidden (D4). Already-hidden and minimized members keep
    // their intent with no setter. Sticky never applies to the prior.
    let prior_members: Vec<crate::workspace::WindowKey> = state
        .workspaces
        .workspace_members(target_output, target_prior)
        .into_iter()
        .collect();
    for key in &prior_members {
        if state.workspaces.is_hidden(key) {
            continue;
        }
        if live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key) {
            continue;
        }
        if let Some(outcome) = fences_hold(state, source_workspace, target_prior) {
            return Some(outcome);
        }
        let Some(expected) = state.member_identity.get(key).cloned() else {
            continue;
        };
        #[cfg(test)]
        {
            let _ = (me, store, dir, expected);
            state.workspaces.set_hidden(key, true);
            candidate_ws.set_hidden(key, true);
            newly_hidden.push(key.clone());
        }
        #[cfg(not(test))]
        {
            let iconic = false;
            let (outcome, _) = workspace_hide_one(state, me, store, dir, key, &expected, iconic);
            if outcome == "hidden" || state.hidden_claims.contains_key(key) {
                state.workspaces.set_hidden(key, true);
                candidate_ws.set_hidden(key, true);
                newly_hidden.push(key.clone());
            } else if outcome == "origin-vanished" || outcome == "identity-changed" {
                drop_member_state(state, key);
            } else if outcome == "scope-excluded" {
                // Out-of-scope prior members stay visible with membership
                // intact (same rule as selects), never a failure.
            } else {
                return Some(outcome);
            }
        }
    }
    // Reveal helper: one hidden-claim member whose new active view permits
    // visibility. Preserves minimized intent via the stored iconic flag
    // (never unminimizes). Mirrors the flag onto the candidate.
    let reveal_one = |state: &mut TileLoop,
                      candidate_ws: &mut crate::workspace::ManagedWorkspaces,
                      key: &crate::workspace::WindowKey|
     -> std::result::Result<(), &'static str> {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            candidate_ws.set_hidden(key, false);
            return Ok(());
        };
        if let Some(outcome) = fences_hold(state, source_workspace, target_prior) {
            return Err(outcome);
        }
        #[cfg(test)]
        {
            let _ = record;
            state.hidden_claims.remove(key);
            state.workspaces.set_hidden(key, false);
            candidate_ws.set_hidden(key, false);
            Ok(())
        }
        #[cfg(not(test))]
        {
            match crate::product_hide::sys::reveal_product_claim_to(
                store,
                me,
                &record.claim,
                record.iconic,
            ) {
                Ok(crate::product_hide::ProductTeardown::Retired) => {
                    state.hidden_claims.remove(key);
                    drop_member_state(state, key);
                    candidate_ws.remove_window(key);
                    Ok(())
                }
                Ok(_) => {
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                    candidate_ws.set_hidden(key, false);
                    Ok(())
                }
                Err(_) => Err("migrate-unverified"),
            }
        }
    };
    // Source refill shows (G37): reveal its hidden members now that the
    // candidate shows the refill.
    if let Some(refill) = refill {
        let refill_members: Vec<crate::workspace::WindowKey> = candidate_ws
            .workspace_members(source_output, refill)
            .into_iter()
            .collect();
        for key in &refill_members {
            // Minimized refill members stay minimized: the reveal path
            // preserves iconic, so they still converge (no skip).
            if reveal_one(state, candidate_ws, key).is_err() {
                return Some("migrate-unverified");
            }
        }
    }
    // Migrated members preserve their pre-existing hidden/minimized intent:
    // no reveal here (the move carries visibility as-is; an active view does
    // not unhide intentional concealment nor unminimize). Membership was
    // fenced per effect above.
    let _ = tail_areas;
    None
}

/// Single causal follow tail for committed workspace migrations (D6): follow
/// the moved active client only. Empty and sticky-active migrations (no
/// active window) take no setter. A carried fullscreen (live re-read
/// immediately before focus, never the stale pre-plan row) or a minimized
/// client takes no focus setter and only records foreground. Otherwise one
/// [`actuate_focus`] attempt decides; a focus failure after the committed
/// verified transfer reports the focus outcome with the transfer retained.
/// Fresh topology is re-read immediately before the overlay/focus reads so a
/// late fullscreen drift cannot slip into a focus actuation.
#[allow(clippy::too_many_arguments)]
fn migration_follow_tail(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    members: &BTreeSet<crate::workspace::WindowKey>,
    migrate_rows: &[crate::workspace_owner::MigrateRow],
    active: Option<tiler_core::directional::WindowId>,
    focus_fulls_fallback: &[Rect],
    log_migrate: &dyn Fn(&'static str, serde_json::Value),
) -> &'static str {
    let Some(active) = active else {
        log_migrate("migrate-applied", serde_json::json!({"focus": "none"}));
        return "migrate-applied";
    };
    let key = members.iter().find(|key| {
        state
            .member_tokens
            .get(*key)
            .is_some_and(|t| t == &active.0)
    });
    // Fresh fullscreen read immediately before any focus setter: the tail
    // topology is re-read here (never the pre-plan row, never stale fulls),
    // so a late fullscreen drift suppresses instead of focusing.
    let fresh_areas = fresh_cross_areas();
    let owned: Vec<Rect>;
    let focus_fulls: &[Rect] = if let Some(areas) = fresh_areas.as_ref() {
        owned = areas.iter().map(|a| a.full).collect();
        &owned
    } else {
        focus_fulls_fallback
    };
    let row_fullscreen = migrate_rows
        .iter()
        .find(|row| row.token == active.0)
        .map(|row| row.fullscreen)
        .unwrap_or(false);
    let fullscreen_live = key
        .map(|key| {
            let (_, live_full_opt) = live_overlay_flags(key.hwnd, focus_fulls);
            live_full_opt.unwrap_or(row_fullscreen)
        })
        .unwrap_or(row_fullscreen);
    if fullscreen_live {
        if let Some(key) = key {
            state.workspaces.note_foreground(key);
        }
        log_migrate(
            "migrate-applied",
            serde_json::json!({"window": active.0, "focus": "suppressed"}),
        );
        return "migrate-applied";
    }
    let minimized =
        key.is_some_and(|key| live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key));
    if minimized {
        log_migrate(
            "migrate-applied",
            serde_json::json!({"window": active.0, "focus": "suppressed"}),
        );
        return "migrate-applied";
    }
    let actuation = actuate_focus(state, me, focus_fulls, observed, retained, &active.0);
    if actuation.outcome != "focus-ok" {
        log_migrate(
            actuation.outcome,
            serde_json::json!({
                "window": active.0,
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
            }),
        );
        return actuation.outcome;
    }
    note_focus_advance(state, observed, retained, &active.0);
    if let Some(key) = key {
        state.workspaces.note_foreground(key);
    }
    log_migrate(
        "migrate-applied",
        serde_json::json!({"window": active.0, "setter_accepted": true}),
    );
    "migrate-applied"
}

/// Run one pinned whole-workspace migration through the commit-last native
/// transfer (REQ-WS-12, decisions D1-D9/G37).
///
/// The caller already pinned `(source_output, source_workspace)` as the chord
/// output's active workspace and `target_output` as the adjacent output with
/// `target_prior` as its current workspace; all three stay frozen through
/// transfer and follow (never re-ranked). The target migration domain carries
/// the SAME workspace id on the target output (the prior stays listed and
/// hidden, D4). This snapshots migration fences, dispatches the
/// DISTINCT `MigrateWorkspace` command on a candidate Engine clone with the
/// retained domain (source refill reads the live
/// `core.migration_source_refill` mode), then actuates commit-last exactly
/// like the explicit output-send foundation: fresh topology, one combined
/// apply, its readback, float translation moves, and the arrival proof all
/// verify before the project reassignment and the authoritative Engine
/// publish once. Sticky members stay homed on the source refill; floats ride
/// with class/origin via native output remap (move-only, never a resize).
/// Follow foregrounds the moved active client once (suppressed for a carried
/// fullscreen or a minimized client, which stay minimized); empty and
/// sticky-active migrations take no focus setter. Minimized clients stay
/// minimized. Failures stop narrowly with no replay and no guessed rollback;
/// the retained Engine plan stands and the next observation converges. A
/// focus failure after a committed verified transfer reports the focus
/// outcome with the transfer retained, matching foundation semantics.
/// Returns the workspace-migrate outcome token and logs the full action line.
#[allow(clippy::too_many_arguments)]
fn run_workspace_migrate(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    hint_cx: &mut HintCx,
    source_output: &str,
    source_workspace: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    migrate_rows: &[crate::workspace_owner::MigrateRow],
    target_output: &str,
    target_prior: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_migrate = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-migrate"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    let Some((source_domain_base, source_key)) = workspace_domain_for(
        source_output,
        source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    let Some((target_domain_base, target_key)) = workspace_domain_for(
        target_output,
        source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    // Fresh user-safety gate before any Engine or project mutation.
    if suspend_read(state, me, fulls).veto.block {
        log_migrate("suspended", serde_json::json!({}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_migrate("elevated-foreground", serde_json::json!({}));
        return "elevated-foreground";
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        log_migrate("workspace-disabled", serde_json::json!({}));
        return "workspace-disabled";
    }
    // Whole-member scope/lifetime gates before any Engine or project
    // mutation; tail-time fences re-verify live before commit. One drifted
    // member refuses the whole migration (never a partial carry, never a
    // skip): identity drift drops that member's runtime state first.
    let members = state
        .workspaces
        .workspace_members(source_output, source_workspace);
    for key in &members {
        if let Some(stored) = state.member_identity.get(key) {
            if !scope_allows(&state.scope, &stored.exe_path) {
                log_migrate("scope-excluded", serde_json::json!({}));
                return "scope-excluded";
            }
            if !hosted_gate_allows(&stored.exe_path, key.hwnd, stored.pid, &state.scope_hosts) {
                log_migrate("scope-excluded", serde_json::json!({}));
                return "scope-excluded";
            }
        }
        let live_tag = live_member_tag(key.hwnd);
        let lifetime_ok = state.member_tags.get(key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        if !lifetime_ok {
            drop_member_state(state, key);
            log_migrate("identity-changed", serde_json::json!({}));
            return "identity-changed";
        }
    }
    // Focused client binds to a migrating non-sticky member only; empty and
    // sticky-active migrations carry no invented focus (native output
    // switching only, D6).
    let sticky_keys: BTreeSet<crate::workspace::WindowKey> = members
        .iter()
        .filter(|key| state.sticky.contains_key(*key))
        .cloned()
        .collect();
    let focused_token = state
        .focused_token(observed)
        .map(|window| window.0.clone())
        .filter(|token| {
            migrate_rows
                .iter()
                .any(|row| &row.token == token && !row.sticky)
        })
        .unwrap_or_default();
    // Carried pre-plan fences over the pinned pair: the target current view
    // is the pinned prior, the source refill resolves at commit time through
    // the shared G-37 selector (never an ad hoc fallback).
    let source_area = areas.iter().find(|a| a.device == source_output);
    let target_area = areas.iter().find(|a| a.device == target_output);
    let (Some(source_area), Some(target_area)) = (source_area, target_area) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    let source_bounds = match tiling_domain_bounds_with(source_area.work, state.outer_gap) {
        Some(bounds) => bounds,
        None => {
            log_migrate("unknown-output", serde_json::json!({}));
            return "unknown-output";
        }
    };
    let target_bounds = match tiling_domain_bounds_with(target_area.work, state.outer_gap) {
        Some(bounds) => bounds,
        None => {
            log_migrate("unknown-output", serde_json::json!({}));
            return "unknown-output";
        }
    };
    let snapshot = crate::workspace_owner::MigrateFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: source_output.to_owned(),
        target_output: target_output.to_owned(),
        moved_workspace: source_workspace.to_owned(),
        source_refill: None,
        target_prior: target_prior.to_owned(),
        source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
        target_tiled: state.workspaces.is_tiled(target_output, target_prior),
        direction: intent.direction,
        source_full: source_area.full,
        target_full: target_area.full,
        source_bounds,
        target_bounds,
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
        member_count: migrate_rows.len(),
    };
    let membership_ok = members.iter().all(|key| {
        state.workspaces.member_loc(key).is_some_and(|fresh| {
            fresh.output == source_output && fresh.workspace == source_workspace
        })
    });
    let live = crate::workspace_owner::MigrateFenceLive {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_active: state.workspaces.active_id(source_output),
        target_active: state.workspaces.active_id(target_output),
        source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
        target_tiled: state.workspaces.is_tiled(target_output, target_prior),
        topology_ok: fresh_cross_areas().is_some_and(|fresh| {
            cross_topology_fresh(
                &crate::workspace_owner::migrate_snapshot_for_drag(&snapshot),
                &fresh,
            )
        }),
        scope_ok: true,
        lifetime_ok: true,
        membership_ok,
        views_ok: state.workspaces.active_id(source_output).as_deref() == Some(source_workspace)
            && state.workspaces.active_id(target_output).as_deref() == Some(target_prior),
        owner_ok: true,
    };
    if let Some(outcome) = crate::workspace_owner::migrate_fences_hold(&snapshot, &live) {
        if outcome == "identity-changed" {
            for key in &members {
                drop_member_state(state, key);
            }
        }
        log_migrate(outcome, serde_json::json!({}));
        return outcome;
    }
    // D4 untiled source: preserve native frames via move-only native remap
    // with no tiled geometry and no Engine dispatch (the Engine is normally
    // absent after release). The candidate Engine stays untouched; the empty
    // plan below carries only the remembered follow focus. A stale retained
    // source session refuses before effects below so the project commit can
    // never straddle a retained Engine domain.
    let source_tiled = state.workspaces.is_tiled(source_output, source_workspace);
    // Candidate Engine for the whole-workspace route: gap adopts and the
    // migrate dispatch below all run on the clone. The authoritative Engine
    // publishes once, only after the verified tails commit.
    let mut candidate = state.engine.clone();
    let (reply, plan) = if !source_tiled {
        let _ = hint_cx;
        let active_window = if focused_token.is_empty() {
            None
        } else {
            Some(tiler_core::directional::WindowId(focused_token.clone()))
        };
        let empty = tiler_core::boundary::MigrateWorkspacePlan {
            base_revision: revision_for(state, source_output, source_workspace),
            direction: intent.direction,
            source: source_key.clone(),
            target: target_key.clone(),
            geometry: Vec::new(),
            focus_domain: None,
            focus_leaf: None,
            active_window,
            members: migrate_rows.iter().filter(|r| !r.sticky).count(),
            floats: migrate_rows
                .iter()
                .filter(|r| !r.sticky && r.floating)
                .count(),
            preconditions: Vec::new(),
        };
        (
            tiler_core::boundary::CoreReply::MigrateWorkspace(empty.clone()),
            empty,
        )
    } else {
        {
            let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let source_fp = crate::tiling::fingerprint(
                &source_rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let focused = if focused_token.is_empty() {
                None
            } else {
                Some(WindowId(focused_token.clone()))
            };
            adopt_gaps_on_candidate(
                &mut candidate,
                state,
                &source_domain_base,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
                revision_for(state, source_output, source_workspace),
                source_fp,
                correlation,
            );
        }
        let combined_fp = {
            let mut pairs: Vec<(String, Rect)> = source_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            crate::tiling::fingerprint(&pairs)
        };
        // The shared hint-query budget is source-only here: the target domain
        // carries no rows (the Engine refuses carried target residue), so the
        // caller's `hint_cx` stays unconsumed by this route.
        let _ = hint_cx;
        let Some(mut migrate_event) = crate::workspace_owner::build_migrate_event(
            &state.owner,
            &state.generation,
            correlation,
            revision_for(state, source_output, source_workspace),
            combined_fp,
            (source_domain_base.clone(), source_key.clone()),
            (target_domain_base.clone(), target_key.clone()),
            migrate_rows,
            &focused_token,
            direction_name(intent.direction),
            state.outer_gap,
        ) else {
            log_migrate("refused", serde_json::json!({}));
            return "refused";
        };
        let reply = {
            stamp_engine_event(state, &mut migrate_event);
            candidate.handle(&migrate_event)
        };
        let plan = match &reply {
            tiler_core::boundary::CoreReply::MigrateWorkspace(plan) => plan.clone(),
            tiler_core::boundary::CoreReply::Rejected { kind, .. } => {
                log_migrate(kind, serde_json::json!({}));
                return kind;
            }
            tiler_core::boundary::CoreReply::Diverged(reason) => {
                let outcome = reason.as_str();
                log_migrate(outcome, serde_json::json!({}));
                return outcome;
            }
            tiler_core::boundary::CoreReply::SnapshotInvalid { detail, .. } => {
                log_migrate(detail, serde_json::json!({}));
                return detail;
            }
            _ => {
                log_migrate("refused", serde_json::json!({}));
                return "refused";
            }
        };
        (reply, plan)
    };
    // Untiled sources carry no Engine domain: a stale retained source session
    // (release missed) refuses before any native effect so the project commit
    // below can never straddle a retained Engine domain. Fail-closed with
    // reconcile, no replay, no publication.
    if !source_tiled && state.engine.session(&source_key).is_some() {
        log_migrate("unverified", serde_json::json!({}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "unverified";
    }
    // Fresh topology before any native effect: the pinned pair must still
    // touch with identical FULL rectangles and inset bounds (never
    // re-ranked off the relocated members).
    let drag_snapshot = crate::workspace_owner::migrate_snapshot_for_drag(&snapshot);
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_migrate("migrate-unverified", serde_json::json!({}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "migrate-unverified";
    };
    if !cross_topology_fresh(&drag_snapshot, &fresh_areas) {
        log_migrate("migrate-unverified", serde_json::json!({}));
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // One combined apply for tiled plain members only: the Engine plan
    // reflows the relocated domain in DPI-aware physical coordinates with no
    // rescaling. Core still projects retained tiled leaves for carried
    // overlays (`fit_excluded` never removes them), so overlays are excluded
    // here by inspection and ride the native D8 tail below (move-only, never
    // a resize/refit/restore). Floats, hidden and minimized members converge
    // Engine membership but never take geometry writes here.
    let overlay_tokens: HashSet<String> = migrate_rows
        .iter()
        .filter(|row| !row.sticky && (row.fullscreen || row.maximized))
        .map(|row| row.token.clone())
        .collect();
    let float_tokens_migrate: HashSet<String> = migrate_rows
        .iter()
        .filter(|row| !row.sticky && row.floating)
        .map(|row| row.token.clone())
        .collect();
    let mut writable = writable_tokens(state, source_output, source_workspace, observed);
    writable
        .retain(|token| !overlay_tokens.contains(token) && !float_tokens_migrate.contains(token));
    // Untiled source takes no tiled geometry: every non-sticky member moves
    // natively move-only below (native frames preserved, no Engine dispatch).
    if !source_tiled {
        writable.clear();
    }
    // Untiled sources skip the combined tiled apply entirely (no Engine
    // geometry to write or read back); tiled sources run it exactly once.
    if !source_tiled {
        let _ = &reply;
    }
    let summary = if source_tiled {
        apply_geometry(
            state,
            ApplyInput {
                me,
                fulls: &fresh_fulls,
                reply: &reply,
                observed,
                op: "workspace-migrate",
                tick,
                correlation: correlation_str,
                skipped: Vec::new(),
                writable: &writable,
                output_token: state.workspaces.output_token(source_output),
                workspace_token: state
                    .workspaces
                    .workspace_token(source_output, source_workspace),
                revision: revision_for(state, source_output, source_workspace),
            },
        )
    } else {
        // No tiled writes were issued, so there is nothing to read back:
        // report a clean empty apply for the gates below.
        Some(ApplySummary {
            applied: 0,
            mismatched: 0,
            readback_ok: true,
            veto: None,
        })
    };
    let Some(summary) = summary else {
        log_migrate("migrate-unverified", serde_json::json!({}));
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    };
    if !summary.readback_ok {
        log_migrate("migrate-unverified", serde_json::json!({}));
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    }
    if summary.mismatched > 0 {
        // Partial native writes converge on observation without setter
        // replay: reconcile the fresh domains and report, never a guessed
        // rollback.
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("migrate-mismatch", serde_json::json!({}));
        return "migrate-mismatch";
    }
    // ALL-member native output remap (D7/D8, move-only, never a resize or
    // refit): every migrating non-sticky member outside the tiled-plain
    // writable set remaps by the FULL-rectangle origin delta onto the target
    // output. Plain floats and fullscreen overlays move with `SetWindowPos`
    // NOSIZE; maximized overlays remap placement normal rect preserving
    // `showCmd` (still maximized, reserved slot kept); minimized members
    // remap placement normal rect preserving `showCmd` (still minimized);
    // hidden members remap placement preserving hidden state. Sticky members
    // stay on the source and take no move. Each setter re-verifies fresh
    // identity/lifetime/scope/topology/view/overlay fences first; each
    // arrival verifies monitor/identity (overlays) or output+size (floats)
    // or output+show-state (concealed), never the old exact rect. Any
    // failure stops narrowly with reconcile, no replay, no publication; the
    // next observation converges the partial physical moves.
    let row_by_token: HashMap<&str, &crate::workspace_owner::MigrateRow> = migrate_rows
        .iter()
        .map(|row| (row.token.as_str(), row))
        .collect();
    let mut transfers: Vec<(crate::workspace::WindowKey, String)> = Vec::new();
    for key in &members {
        let Some(token) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        let Some(row) = row_by_token.get(token.as_str()) else {
            continue;
        };
        if row.sticky {
            continue;
        }
        if writable.contains(token.as_str()) {
            continue;
        }
        transfers.push((key.clone(), token));
    }
    transfers.sort_by(|a, b| a.1.cmp(&b.1));
    for (key, token) in &transfers {
        let Some(row) = row_by_token.get(token.as_str()).copied() else {
            continue;
        };
        // Per-effect fences before the setter: scope, lifetime, topology,
        // views, owner, overlay. Any drift refuses with reconcile and no
        // replay.
        let scope_ok = state.member_identity.get(key).is_some_and(|stored| {
            scope_allows(&state.scope, &stored.exe_path)
                && hosted_gate_allows(&stored.exe_path, key.hwnd, stored.pid, &state.scope_hosts)
        });
        if !scope_ok {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("scope-excluded", serde_json::json!({"window": token}));
            return "scope-excluded";
        }
        let lifetime_ok = state.member_tags.get(key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(
                stored,
                live_member_tag(key.hwnd).as_deref(),
            )
        });
        if !lifetime_ok {
            drop_member_state(state, key);
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("identity-changed", serde_json::json!({"window": token}));
            return "identity-changed";
        }
        let Some(tail_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        };
        if !cross_topology_fresh(&drag_snapshot, &tail_areas) {
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            reconcile_tick(state, me, &tail_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        if state.workspaces.active_id(source_output).as_deref() != Some(source_workspace)
            || state.workspaces.active_id(target_output).as_deref() != Some(target_prior)
        {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("view-changed", serde_json::json!({"window": token}));
            return "view-changed";
        }
        if state
            .engine
            .owner()
            .is_none_or(|owner| owner.as_str() != snapshot.owner.as_str())
            || state
                .engine
                .generation()
                .is_none_or(|generation| generation.as_str() != snapshot.generation.as_str())
        {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("owner-changed", serde_json::json!({"window": token}));
            return "owner-changed";
        }
        let hidden = state.workspaces.is_hidden(key);
        let minimized = live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key);
        // Overlay drift: a visible member whose live flags no longer match
        // the carried row authorizes no move. Hidden/minimized members keep
        // no live frame (`None` fullscreen), so only the maximized bit is
        // fenced there against the retained row.
        if hidden || minimized {
            let (live_max, _) = live_overlay_flags(key.hwnd, &fresh_fulls);
            if live_max != row.maximized {
                reconcile_tick(state, me, &fresh_fulls, &tail_areas);
                log_migrate("migrate-unverified", serde_json::json!({"window": token}));
                return "migrate-unverified";
            }
        } else {
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, &fresh_fulls);
            if live_max != row.maximized
                || live_full_opt.unwrap_or(row.fullscreen) != row.fullscreen
            {
                reconcile_tick(state, me, &fresh_fulls, &tail_areas);
                log_migrate("migrate-unverified", serde_json::json!({"window": token}));
                return "migrate-unverified";
            }
        }
        // Source frame: live visible frame when present, else the carried
        // normal frame (retained/member/float snapshot) for concealed
        // members. No frame is never fabricated: missing refuses.
        let frame = observed
            .iter()
            .find(|window| window.token == *token)
            .map(|window| window.visible)
            .or_else(|| retained.iter().find(|r| r.key == *key).and_then(|r| r.rect))
            .or_else(|| state.member_rects.get(token).copied())
            .or_else(|| state.float_rects.get(token).copied())
            .unwrap_or(row.rect);
        let moved = crate::workspace_owner::translate_rect_to_output(
            frame,
            snapshot.source_full,
            snapshot.target_full,
        );
        if output_for_rect(&tail_areas, &moved) != target_output {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        let setter_ok = if hidden {
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            remap_placement_to_output(key.hwnd, moved, "hidden", target_work, target_full, basis)
        } else if minimized {
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            remap_placement_to_output(
                key.hwnd,
                moved,
                "minimized",
                target_work,
                target_full,
                basis,
            )
        } else if row.fullscreen || row.maximized {
            let source_area = tail_areas.iter().find(|a| a.device == source_output);
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let source_work = source_area.map(|a| a.work).unwrap_or(snapshot.source_full);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let source_full = source_area.map(|a| a.full).unwrap_or(snapshot.source_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            move_overlay_to_output(
                key.hwnd,
                moved,
                row.maximized,
                source_full,
                target_full,
                source_work,
                target_work,
                basis,
            )
        } else if row.floating || !source_tiled {
            // Untiled plain members preserve native frames via the same
            // move-only float path (never a resize/refit); tiled plain
            // members never reach here (they ride the combined apply).
            move_float_to_output(key.hwnd, moved)
        } else {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        };
        if !setter_ok {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        // Arrival proof per member before the next setter: identity still
        // holds (pid/lifetime rechecked live) and the fresh native physical
        // output verifies after the actual setter (never the pre-move stable
        // cache, never translation math). Visible floats verify output plus
        // preserved size; visible overlays verify monitor/identity only,
        // never the old exact rect (D8); concealed members verify fresh
        // placement output plus preserved show-state (still
        // minimized/hidden, maximized slot kept).
        #[cfg(test)]
        let present = state.member_tags.contains_key(key);
        #[cfg(not(test))]
        let present = unsafe { IsWindow(key.hwnd as isize as HWND) } != 0;
        if !present {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        let arrival_ok = if hidden {
            // Concealed hidden arrival: fresh placement readback converted to
            // screen must sit on the target output (physical proof, never
            // translation math) and the project hidden intent must hold.
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            state.workspaces.is_hidden(key)
                && fresh_arrival_placement(key.hwnd, target_work, target_full, basis)
                    .is_some_and(|placed| output_for_rect(&tail_areas, &placed) == target_output)
        } else if minimized {
            // Concealed minimized arrival: still minimized plus fresh
            // placement converted to screen on the target.
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            (live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key))
                && fresh_arrival_placement(key.hwnd, target_work, target_full, basis)
                    .is_some_and(|placed| output_for_rect(&tail_areas, &placed) == target_output)
        } else if (row.floating || !source_tiled) && !row.fullscreen && !row.maximized {
            // Visible float (or untiled plain via the move-only path)
            // arrival: fresh native visible frame preserves size on the target
            // (causal readback after the setter, never stale cache).
            fresh_arrival_rect(key.hwnd).is_some_and(|fresh| {
                fresh.w == frame.w
                    && fresh.h == frame.h
                    && output_for_rect(&tail_areas, &fresh) == target_output
            })
        } else {
            // Overlay (maximized/fullscreen) arrival: fresh native visible
            // frame on the target monitor plus the live overlay flag, never
            // the old exact rect (native re-fit may change the maximized
            // frame size on the target work area).
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, &tail_fulls);
            let flag_ok = live_max == row.maximized
                && live_full_opt.unwrap_or(row.fullscreen) == row.fullscreen;
            flag_ok
                && fresh_arrival_rect(key.hwnd)
                    .is_some_and(|fresh| output_for_rect(&tail_areas, &fresh) == target_output)
        };
        if !arrival_ok {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
    }
    // Required arrival proof before claiming success: every migrating tiled
    // plain member with a planned write must read back at its planned rect on
    // the destination monitor. Overlay/float/concealed arrivals verified per
    // member above, never by their old frame (D8); only writable tiled plain
    // writes gate here. A failed proof reports unverified with reconcile, no
    // commit, no revert, and no replay; the next observation converges.
    let writes = crate::workspace_owner::planned_writes(&reply);
    if let Some(writes) = writes {
        for write in &writes {
            if !writable.contains(write.window.0.as_str()) {
                continue;
            }
            let Some(key) = members.iter().find(|key| {
                state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            }) else {
                continue;
            };
            let effective = if write.overconstrained {
                let hints = state
                    .hint_logged
                    .get(write.window.0.as_str())
                    .copied()
                    .unwrap_or(WindowSizeHints::none());
                crate::tiling::overconstrained_effective(write.rect, hints)
            } else {
                write.rect
            };
            // Tiled plain arrival: fresh native frame after the combined
            // apply must equal the planned rect on the destination monitor
            // (causal readback, never the pre-move stable cache).
            let Some(visible) = fresh_arrival_rect(key.hwnd) else {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate(
                    "migrate-unverified",
                    serde_json::json!({"window": write.window.0}),
                );
                return "migrate-unverified";
            };
            if visible != effective || output_for_rect(&fresh_areas, &visible) != target_output {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate(
                    "migrate-unverified",
                    serde_json::json!({"window": write.window.0}),
                );
                return "migrate-unverified";
            }
        }
    }
    // Final live fences after effects, before logical commit: suspend,
    // elevated, and the full pinned-snapshot fence set re-verified live
    // against the current fresh topology (current target_scope fences after
    // effects). Any drift refuses with reconcile, no commit, no replay, and
    // no success claim; both retained authorities stand and the next
    // observation converges.
    if suspend_read(state, me, &fresh_fulls).veto.block {
        log_migrate("suspended", serde_json::json!({}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_migrate("elevated-foreground", serde_json::json!({}));
        return "elevated-foreground";
    }
    {
        let Some(tail_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        };
        if !cross_topology_fresh(&drag_snapshot, &tail_areas) {
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            reconcile_tick(state, me, &tail_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        }
        let membership_ok = members.iter().all(|key| {
            state.workspaces.member_loc(key).is_some_and(|fresh| {
                fresh.output == source_output && fresh.workspace == source_workspace
            })
        });
        let views_ok = state.workspaces.active_id(source_output).as_deref()
            == Some(source_workspace)
            && state.workspaces.active_id(target_output).as_deref() == Some(target_prior);
        let scope_ok = members.iter().all(|key| {
            state.member_identity.get(key).is_some_and(|stored| {
                scope_allows(&state.scope, &stored.exe_path)
                    && hosted_gate_allows(
                        &stored.exe_path,
                        key.hwnd,
                        stored.pid,
                        &state.scope_hosts,
                    )
            })
        });
        let lifetime_ok = members.iter().all(|key| {
            state.member_tags.get(key).is_some_and(|stored| {
                crate::workspace_owner::visible_lifetime_ok(
                    stored,
                    live_member_tag(key.hwnd).as_deref(),
                )
            })
        });
        let owner_ok = state
            .engine
            .owner()
            .is_some_and(|owner| owner.as_str() == snapshot.owner.as_str())
            && state
                .engine
                .generation()
                .is_some_and(|generation| generation.as_str() == snapshot.generation.as_str());
        let live = crate::workspace_owner::MigrateFenceLive {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_active: state.workspaces.active_id(source_output),
            target_active: state.workspaces.active_id(target_output),
            source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
            target_tiled: state.workspaces.is_tiled(target_output, target_prior),
            topology_ok: true,
            scope_ok,
            lifetime_ok,
            membership_ok,
            views_ok,
            owner_ok,
        };
        if let Some(outcome) = crate::workspace_owner::migrate_fences_hold(&snapshot, &live) {
            if outcome == "identity-changed" {
                for key in &members {
                    drop_member_state(state, key);
                }
            }
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate(outcome, serde_json::json!({}));
            return outcome;
        }
    }
    // Commit-last: the project reassignment runs on a clone through the
    // stable-ID ring primitive (source refill reads the same live G-37
    // mode), then both authorities publish once only after the membership
    // plus both-view verification holds. A refusal below publishes nothing.
    let refill_mode = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.migration_source_refill_mode())
        .unwrap_or_default();
    let mut candidate_ws = state.workspaces.clone();
    let Some(outcome) = candidate_ws.migrate_workspace_excluding(
        source_output,
        target_output,
        refill_mode,
        &sticky_keys,
    ) else {
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("unverified", serde_json::json!({}));
        return "unverified";
    };
    let migrating: BTreeSet<crate::workspace::WindowKey> =
        members.difference(&sticky_keys).cloned().collect();
    // Empty-source convergence: when the primitive reports no refill the
    // source keeps an empty order with no current view. Ordinary
    // trailing-empty lifecycle (minimum-two, one empty) converges on the
    // candidate before verification can publish; no ad-hoc empty-source
    // success. Sticky residue re-homes to the converged refill below.
    if outcome.source_refill.is_none() {
        let displaced: Vec<String> = candidate_ws
            .displaced_snapshot()
            .values()
            .flat_map(|r| r.workspace_ids.clone())
            .collect();
        let (removed, append) =
            candidate_ws.plan_cleanup_excluding(source_output, &[], &displaced, &sticky_keys);
        candidate_ws.apply_cleanup(source_output, &removed, append);
        let converged_refill = candidate_ws.active_id(source_output);
        // Re-home sticky residue left dangling on the moved id (the
        // primitive keeps locations untouched when no refill exists) onto
        // the converged refill, preserving hidden state.
        if let Some(refill) = converged_refill.clone() {
            for key in sticky_keys.iter() {
                if let Some(loc) = candidate_ws.member_loc(key).cloned() {
                    if loc.output == source_output && loc.workspace == outcome.moved_id {
                        candidate_ws.assign(key.clone(), source_output, &refill, loc.hidden);
                    }
                } else if state.workspaces.member_loc(key).is_some_and(|fresh| {
                    fresh.output == source_output && fresh.workspace == source_workspace
                }) {
                    let hidden = state.workspaces.is_hidden(key);
                    candidate_ws.assign(key.clone(), source_output, &refill, hidden);
                }
            }
        }
        let source_members = converged_refill
            .as_deref()
            .map(|refill| candidate_ws.workspace_members(source_output, refill))
            .unwrap_or_default();
        let target_members = candidate_ws.workspace_members(target_output, &outcome.moved_id);
        // Native view publication for the converged refill (same D4/G37
        // staging as the non-empty path): hide the prior, reveal the
        // converged refill before verification can publish.
        {
            let Some(view_areas) = fresh_cross_areas() else {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate("migrate-unverified", serde_json::json!({}));
                return "migrate-unverified";
            };
            if let Some(view_outcome) = migration_view_effects(
                state,
                me,
                store,
                dir,
                &mut candidate_ws,
                source_output,
                converged_refill.as_deref(),
                target_output,
                target_prior,
                &migrating,
                &snapshot,
                &view_areas,
            ) {
                reconcile_tick(state, me, &fresh_fulls, &view_areas);
                log_migrate(view_outcome, serde_json::json!({}));
                return view_outcome;
            }
            let prior_members = state
                .workspaces
                .workspace_members(target_output, target_prior);
            let refill_members = converged_refill
                .as_deref()
                .map(|refill| candidate_ws.workspace_members(source_output, refill))
                .unwrap_or_default();
            let views_physical = prior_members.iter().all(|key| {
                candidate_ws.is_hidden(key)
                    || live_is_iconic(key.hwnd)
                    || state.minimized_omitted.contains(key)
            }) && refill_members
                .iter()
                .all(|key| !candidate_ws.is_hidden(key));
            if !views_physical {
                reconcile_tick(state, me, &fresh_fulls, &view_areas);
                log_migrate("migrate-unverified", serde_json::json!({}));
                return "migrate-unverified";
            }
        }
        if !crate::workspace_owner::verify_migration_transfer(
            &migrating,
            &sticky_keys,
            &source_members,
            &target_members,
            &outcome.moved_id,
            converged_refill.as_deref(),
            candidate_ws.active_id(source_output).as_deref(),
            candidate_ws.active_id(target_output).as_deref(),
        ) {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("unverified", serde_json::json!({}));
            return "unverified";
        }
        // Publish the lifecycle-converged candidate, then run the single
        // shared follow tail (fresh fullscreen/iconic re-read, no stale row).
        state.workspaces = candidate_ws;
        state.engine = candidate;
        // Item 12 stage B: completed migration views record through the
        // scope contract (global-unique converged refill plus target
        // arrival; local stays on the primitive's own invalidation).
        let moved = outcome.moved_id.clone();
        let converged = converged_refill.clone();
        scope_trailing::note_migration_completed(
            state,
            source_output,
            converged.as_deref(),
            target_output,
            &moved,
        );
        return migration_follow_tail(
            state,
            me,
            observed,
            retained,
            &members,
            migrate_rows,
            plan.active_window.clone(),
            &fresh_fulls,
            &log_migrate,
        );
    }
    let source_members = candidate_ws.workspace_members(
        source_output,
        outcome.source_refill.as_deref().unwrap_or(""),
    );
    let target_members = candidate_ws.workspace_members(target_output, &outcome.moved_id);
    // Native view publication (D4/G37): hide the occupied target prior and
    // reveal the source refill fenced per effect before the membership/view
    // verification can publish. Failures reconcile without replay.
    {
        let Some(view_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        };
        if let Some(view_outcome) = migration_view_effects(
            state,
            me,
            store,
            dir,
            &mut candidate_ws,
            source_output,
            outcome.source_refill.as_deref(),
            target_output,
            target_prior,
            &migrating,
            &snapshot,
            &view_areas,
        ) {
            reconcile_tick(state, me, &fresh_fulls, &view_areas);
            log_migrate(view_outcome, serde_json::json!({}));
            return view_outcome;
        }
        // Both physical views verify before publish: the prior stays listed
        // but hidden (minimized stays minimized), the refill shows.
        let prior_members = state
            .workspaces
            .workspace_members(target_output, target_prior);
        let refill_members = outcome
            .source_refill
            .as_deref()
            .map(|refill| candidate_ws.workspace_members(source_output, refill))
            .unwrap_or_default();
        let views_physical = prior_members.iter().all(|key| {
            candidate_ws.is_hidden(key)
                || live_is_iconic(key.hwnd)
                || state.minimized_omitted.contains(key)
        }) && refill_members
            .iter()
            .all(|key| !candidate_ws.is_hidden(key));
        if !views_physical {
            reconcile_tick(state, me, &fresh_fulls, &view_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        }
    }
    if !crate::workspace_owner::verify_migration_transfer(
        &migrating,
        &sticky_keys,
        &source_members,
        &target_members,
        &outcome.moved_id,
        outcome.source_refill.as_deref(),
        candidate_ws.active_id(source_output).as_deref(),
        candidate_ws.active_id(target_output).as_deref(),
    ) {
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("unverified", serde_json::json!({}));
        return "unverified";
    }
    state.workspaces = candidate_ws;
    state.engine = candidate;
    // Item 12 stage B: completed migration views record through the scope
    // contract (global-unique source refill plus target arrival; local stays
    // on the primitive's own invalidation).
    let refill = outcome.source_refill.clone();
    let moved = outcome.moved_id.clone();
    scope_trailing::note_migration_completed(
        state,
        source_output,
        refill.as_deref(),
        target_output,
        &moved,
    );
    // Verified: the single shared follow tail (D6) runs after publish.
    migration_follow_tail(
        state,
        me,
        observed,
        retained,
        &members,
        migrate_rows,
        plan.active_window.clone(),
        &fresh_fulls,
        &log_migrate,
    )
}

/// Move one migrating float window across outputs (move-only, never a resize
/// or refit): `SetWindowPos` with `SWP_NOSIZE | SWP_NOZORDER |
/// SWP_NOACTIVATE` at the translated frame. No focus, no z-order, no show
/// effects. Under test the move records into the injected float-move log and
/// the ALL-member native-transfer log and reports success without touching
/// native state.
fn move_float_to_output(hwnd_u64: u64, moved: Rect) -> bool {
    #[cfg(test)]
    {
        INJECTED_FLOAT_MOVES.with(|slot| slot.borrow_mut().push((hwnd_u64, moved)));
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, "float")));
        INJECTED_FLOAT_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        let hwnd = hwnd_u64 as isize as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                moved.x,
                moved.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            ) != 0
        }
    }
}

/// Move one migrating overlay window across outputs (D8, move-only, never a
/// resize, refit, restore or remaximize): a visible fullscreen member moves
/// with `SetWindowPos` `SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE` at the
/// translated frame, preserving the native fullscreen frame with no extra
/// geometry or focus writes. A maximized member remaps its live
/// `WINDOWPLACEMENT` normal (restore) rect translated to the target,
/// preserving the original restore size and `showCmd` (still maximized,
/// still holding its reserved tile slot), never restoring first and never
/// writing the observed maximized frame as the restore frame. Under test the
/// maximized arm translates the injected normal boundary with the same pure
/// helper and records the target screen rect as `(hwnd, moved, "overlay")`.
#[allow(clippy::too_many_arguments)]
fn move_overlay_to_output(
    hwnd_u64: u64,
    moved: Rect,
    maximized: bool,
    source_full: Rect,
    target_full: Rect,
    source_work: Rect,
    target_work: Rect,
    workspace_coords: bool,
) -> bool {
    #[cfg(test)]
    {
        if maximized {
            let Some((source_raw, _)) = live_placement_normal(hwnd_u64) else {
                return false;
            };
            let target_raw = translate_placement_normal_to_target(
                source_raw,
                source_work,
                target_work,
                source_full,
                target_full,
                workspace_coords,
            );
            let target_screen =
                placement_raw_to_screen(target_raw, target_work, target_full, workspace_coords);
            INJECTED_NATIVE_TRANSFERS
                .with(|slot| slot.borrow_mut().push((hwnd_u64, target_screen, "overlay")));
            return INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get());
        }
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, "overlay")));
        INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        if maximized {
            return remap_maximized_normal_to_output(
                hwnd_u64,
                source_full,
                target_full,
                source_work,
                target_work,
                workspace_coords,
            );
        }
        let hwnd = hwnd_u64 as isize as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                moved.x,
                moved.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            ) != 0
        }
    }
}

/// Remap one concealed member's `WINDOWPLACEMENT` normal rect across outputs
/// (move-only, never a show-state change): reads the live placement to
/// preserve the original restore size and `showCmd`, converts the target
/// screen frame (`moved`, the FULL-delta translation of the retained normal
/// frame) to the raw `rcNormalPosition` basis with the target monitor's
/// `WORK` plus `FULL` inset delta (workspace coordinates for non-tool
/// top-level windows per the `WINDOWPLACEMENT` contract, screen identity for
/// tool windows), posts async (plus `WPF_ASYNCWINDOWPLACEMENT`, never blocking
/// on a hung owner), so a minimized member stays minimized and a hidden member
/// stays hidden. `kind` is `minimized` or `hidden` and is recorded verbatim in
/// the test seam. Under test records the screen `moved` rect and reports the
/// injected verdict without touching native state.
fn remap_placement_to_output(
    hwnd_u64: u64,
    moved: Rect,
    kind: &'static str,
    target_work: Rect,
    target_full: Rect,
    workspace_coords: bool,
) -> bool {
    #[cfg(test)]
    {
        let _ = (target_work, target_full, workspace_coords);
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, kind)));
        INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        let _ = kind;
        remap_placement_normal_rect(hwnd_u64, moved, target_work, target_full, workspace_coords)
    }
}

/// Production placement remap for minimized/hidden members: `GetWindowPlacement`,
/// keep the original normal restore size and `showCmd`, write the target
/// screen origin converted to the raw basis, post async. Returns `false` on
/// any failure (never a partial show-state change).
#[cfg(not(test))]
fn remap_placement_normal_rect(
    hwnd_u64: u64,
    moved_screen: Rect,
    target_work: Rect,
    target_full: Rect,
    workspace_coords: bool,
) -> bool {
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd, &mut placement) } == 0 {
        return false;
    }
    let orig_w = placement
        .rcNormalPosition
        .right
        .saturating_sub(placement.rcNormalPosition.left);
    let orig_h = placement
        .rcNormalPosition
        .bottom
        .saturating_sub(placement.rcNormalPosition.top);
    if orig_w <= 0 || orig_h <= 0 {
        return false;
    }
    let sized = Rect {
        x: moved_screen.x,
        y: moved_screen.y,
        w: orig_w,
        h: orig_h,
    };
    let raw = screen_to_placement_raw(sized, target_work, target_full, workspace_coords);
    placement.rcNormalPosition.left = raw.x;
    placement.rcNormalPosition.top = raw.y;
    placement.rcNormalPosition.right = raw.x.saturating_add(raw.w);
    placement.rcNormalPosition.bottom = raw.y.saturating_add(raw.h);
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    unsafe { SetWindowPlacement(hwnd, &placement) != 0 }
}

/// Production maximized-overlay normal remap: single live `GetWindowPlacement`
/// snapshot supplies both the normal (restore) rect and the preserved
/// `showCmd`/`flags`, translated to the target with the source/target `FULL`
/// delta plus the per-monitor inset-delta conversion. Preserves the original
/// restore size and `showCmd`, posts async. Never writes the observed
/// maximized frame as the restore frame, never invents a retained fallback,
/// never changes show-state. Returns `false` on any failure.
#[cfg(not(test))]
fn remap_maximized_normal_to_output(
    hwnd_u64: u64,
    source_full: Rect,
    target_full: Rect,
    source_work: Rect,
    target_work: Rect,
    workspace_coords: bool,
) -> bool {
    // Single causal snapshot: one `GetWindowPlacement` read supplies geometry
    // plus `showCmd`/`flags`, so a state change between two reads cannot mix
    // a stale restore frame with a fresh show-state (or vice versa). Basis
    // rides the same live style/parent read; no retained fallback, no
    // observed-maximized-frame reuse.
    let _ = workspace_coords;
    let hwnd = hwnd_u64 as isize as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd, &mut placement) } == 0 {
        return false;
    }
    let w = placement
        .rcNormalPosition
        .right
        .saturating_sub(placement.rcNormalPosition.left);
    let h = placement
        .rcNormalPosition
        .bottom
        .saturating_sub(placement.rcNormalPosition.top);
    if w <= 0 || h <= 0 {
        return false;
    }
    let source_raw = Rect {
        x: placement.rcNormalPosition.left,
        y: placement.rcNormalPosition.top,
        w,
        h,
    };
    let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let top_level = unsafe { GetParent(hwnd) }.is_null();
    let basis = placement_is_workspace_coords(exstyle, top_level);
    let target_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        target_work,
        source_full,
        target_full,
        basis,
    );
    placement.rcNormalPosition.left = target_raw.x;
    placement.rcNormalPosition.top = target_raw.y;
    placement.rcNormalPosition.right = target_raw.x.saturating_add(target_raw.w);
    placement.rcNormalPosition.bottom = target_raw.y.saturating_add(target_raw.h);
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    unsafe { SetWindowPlacement(hwnd, &placement) != 0 }
}

/// Drain one bounded batch of workspace history intents (item 1). Toggle and
/// relative steps work on empty workspaces and unmanaged foreground: they
/// select views, never movers, so no managed-focus gate applies. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (defensive second fence). Fullscreen and elevated foreground
/// gate all history ops like digits. Targets resolve without preactivating
/// (pure `resolve_previous`/`resolve_relative`: no activation, append, or
/// creation); only the verified `workspace_do_select` transition activates,
/// and its observation recording supplies history. Toggle with no recorded
/// change, or a removed/unassigned/out-of-scope previous, settles as
/// `unknown-target` (no-op until the next recorded change, never
/// recreation/ordinal reinterpretation).
#[allow(clippy::too_many_arguments)]
fn workspace_history_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceHistoryIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-history",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let op = intent.op;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        // Pure resolution: no activation, append, or creation. Previous
        // validates its stable id (clears removed/out-of-scope); relative
        // wraps the full scoped ring including trailing empty and >9.
        // Scoped resolvers keep local equivalent; `workspace_do_select`
        // dispatches shared/global-swap native effects.
        let target: Option<String> = match op {
            WorkspaceHistoryOp::Previous => {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.resolve_previous(spaces, &output)
            }
            WorkspaceHistoryOp::Prev => {
                state
                    .scope_owner
                    .resolve_relative(&state.workspaces, &output, -1)
            }
            WorkspaceHistoryOp::Next => {
                state
                    .scope_owner
                    .resolve_relative(&state.workspaces, &output, 1)
            }
        };
        match target {
            Some(id) => {
                // REQ-WS-09 departure memory, same as the numbered select.
                let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
                if let Some(source_id) = state.workspaces.active_id(&output)
                    && let Some(origin) = intent.origin.as_ref()
                    && let Some(token) = remember_select_departure(
                        state,
                        origin,
                        &output,
                        &source_id,
                        foreground_hwnd,
                        &observed,
                        &retained,
                    )
                {
                    log_json_at(
                        &state.log_path.clone(),
                        serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                    );
                }
                let effect = workspace_do_select(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &output,
                    &id,
                    &ctx,
                    None,
                    false,
                    false,
                );
                log_json_at(
                    &log_path,
                    workspace_history_log(state, tick, op.as_str(), edge.as_str(), effect.outcome),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op: op.as_str(),
                        index: 0,
                        edge: edge.as_str(),
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: 0,
                            target_plan_ms: effect.plan_ms,
                            source_geometry_ms: 0,
                            target_geometry_ms: effect.geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: None,
                        target: effect.geometry.as_ref(),
                    },
                );
            }
            None => {
                log_json_at(
                    &log_path,
                    workspace_history_log(
                        state,
                        tick,
                        op.as_str(),
                        edge.as_str(),
                        "unknown-target",
                    ),
                );
            }
        }
    }
}

/// Bounded history log line: opaque output/workspace tokens plus
/// op/edge/outcome only. No HWNDs, tokens, titles, or app data.
fn workspace_history_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace-history",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Bounded workspace log line: opaque output/workspace tokens plus
/// op/index/edge/outcome only. No HWNDs, tokens, titles, or app data.
fn workspace_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    index: u8,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "index": index,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Stable per-action context: one opaque correlation for the whole select /
/// send / follow lifecycle, even when inner reconcile ticks increment. Times
/// are monotonic `Instant`s: `queued_at` is the cheap callback stamp from the
/// existing hook event (no logging/syscalls in the callback), `start` is the
/// dispatch instant on the loop thread.
struct ActionCtx {
    correlation: String,
    tick: u64,
    queued_at: Instant,
    start: Instant,
}

/// Settled effect of one verified select transition: membership outcome
/// plus honest per-phase geometry and focus evidence. `geometry` is `None`
/// when no geometry pass ran (empty target, observation failure, early
/// refusal, non-plan Engine reply with readback not run); otherwise it
/// carries the verified verdict so `ok` never implies applied.
/// `transition_ms` covers dispatch to activation (hide+reveal+activate,
/// measured before the post-reveal observation); `observation_ms` covers the
/// post-reveal observation alone. `action_ms` on the log line is the terminal
/// dispatch-to-geometry-completion offset.
struct SelectEffect {
    outcome: &'static str,
    geometry: Option<ApplySummary>,
    focus: &'static str,
    transition_ms: u64,
    observation_ms: u64,
    geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
    plan_ms: u64,
    source_workspace: String,
    target_workspace: String,
}

/// Settled effect of one send+follow action: membership outcome plus honest
/// source (pre-hide reflow) and target (post-reveal) geometry evidence.
/// `source`/`target` are `None` when that pass never ran. Plan and geometry
/// durations stay split per domain, never collapsed. Tokens are captured
/// before cleanup so a removed source still logs its opaque identity.
struct SendEffect {
    outcome: &'static str,
    focus: &'static str,
    source: Option<ApplySummary>,
    target: Option<ApplySummary>,
    transition_ms: u64,
    observation_ms: u64,
    source_geometry_ms: u64,
    target_geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
    source_plan_ms: u64,
    target_plan_ms: u64,
    source_workspace: String,
    target_workspace: String,
}

/// Grouped phase timings for one bounded action line: dispatch offsets plus
/// per-phase durations. Source/target plan and geometry stay distinct.
struct ActionTimings {
    transition_ms: u64,
    observation_ms: u64,
    source_plan_ms: u64,
    target_plan_ms: u64,
    source_geometry_ms: u64,
    target_geometry_ms: u64,
    hide_ms: u64,
    reveal_ms: u64,
    focus_ms: u64,
}

/// One bounded action summary: opaque correlation plus monotonic
/// queue/action/transition/geometry durations and per-phase
/// applied/mismatched/readback states. A phase that never ran serializes as
/// null (outcome carries empty vs deferred vs failed), never zeros implying
/// applied; a vetoed pass carries its bounded `veto` reason.
struct ActionLine<'a> {
    ctx: &'a ActionCtx,
    op: &'a str,
    index: u8,
    edge: &'a str,
    outcome: &'a str,
    focus: &'a str,
    timings: &'a ActionTimings,
    source_workspace: &'a str,
    target_workspace: &'a str,
    source: Option<&'a ApplySummary>,
    target: Option<&'a ApplySummary>,
}

fn log_workspace_action(_state: &TileLoop, log_path: &Path, line: &ActionLine<'_>) {
    let queue_wait_ms = line
        .ctx
        .start
        .duration_since(line.ctx.queued_at)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let action_ms = line
        .ctx
        .start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let phase = |summary: Option<&ApplySummary>| match summary {
        Some(s) => {
            let mut v = serde_json::json!({
                "applied": s.applied,
                "mismatched": s.mismatched,
                "readback_ok": s.readback_ok,
            });
            match s.veto {
                Some(diag) => {
                    v["veto"] = serde_json::json!({
                        "reason": diag.reason,
                        "desktop": diag.desktop,
                        "visible": diag.visible,
                        "captioned": diag.captioned,
                        "dwm_readable": diag.dwm_readable,
                        "cloaked": diag.cloaked,
                    });
                }
                None => {
                    v["veto"] = serde_json::Value::Null;
                }
            }
            v
        }
        None => serde_json::Value::Null,
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "workspace-action",
            "tick": line.ctx.tick,
            "correlation": line.ctx.correlation,
            "op": line.op,
            "index": line.index,
            "edge": line.edge,
            "outcome": line.outcome,
            "focus": line.focus,
            "queue_wait_ms": queue_wait_ms,
            "action_ms": action_ms,
            "transition_ms": line.timings.transition_ms,
            "observation_ms": line.timings.observation_ms,
            "source_plan_ms": line.timings.source_plan_ms,
            "target_plan_ms": line.timings.target_plan_ms,
            "source_geometry_ms": line.timings.source_geometry_ms,
            "target_geometry_ms": line.timings.target_geometry_ms,
            "hide_ms": line.timings.hide_ms,
            "reveal_ms": line.timings.reveal_ms,
            "focus_ms": line.timings.focus_ms,
            "source_workspace": line.source_workspace,
            "target_workspace": line.target_workspace,
            "source": phase(line.source),
            "target": phase(line.target),
        }),
    );
}

/// True when the foreground window's process is not medium integrity:
/// elevated foreground gates workspace dispatch.
fn foreground_elevated(me: &ProcessIdentity) -> bool {
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED.with(|slot| slot.get()) {
        let _ = me;
        return elevated;
    }
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(foreground, &mut pid);
    }
    if pid == 0 || pid == me.pid {
        return false;
    }
    let Ok(held) = HeldProcess::open(pid) else {
        return true;
    };
    match held.integrity() {
        Ok(rid) => !is_medium_rid(rid),
        Err(_) => true,
    }
}

/// Actual-foreground hidden member selects its workspace. Only a real
/// foreground change to an exactly verified hidden member switches; the
/// `Wake` event alone never does. Returns the settled select effect or `None`.
fn poll_foreground_workspace(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
) -> Option<SelectEffect> {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 || foreground == state.last_foreground {
        return None;
    }
    state.last_foreground = foreground;
    // Keyed by full member identity: find the HWND's key, then guard full
    // equality before any lookup so a recycled HWND never selects.
    let key = state
        .hidden_claims
        .keys()
        .find(|k| k.hwnd == foreground)
        .cloned()?;
    let record = state.hidden_claims.get(&key).cloned()?;
    // Exact verification: live PID, full process identity, exact nonce.
    let snap = crate::product_hide::sys::query_recovery(foreground, me).ok()?;
    if !crate::workspace_owner::member_matches(
        &key,
        foreground,
        snap.pid,
        &snap.process.process_creation,
    ) || snap.process != record.claim.process
        || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
    {
        return None;
    }
    // Externally foregrounded: this is genuinely the member's workspace.
    let loc = state.workspaces.member_loc(&key).cloned()?;
    if !loc.hidden {
        return None;
    }
    let output = loc.output.clone();
    let workspace = loc.workspace.clone();
    // No queued chord here: the external foreground is the trigger, so the
    // queue wait is zero by construction.
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    let effect = workspace_do_select(
        state, me, store, dir, fulls, areas, observed, &output, &workspace, &ctx, None, false,
        false,
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: "select",
            index: 0,
            edge: "foreground",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
    Some(effect)
}

/// Externally revealed hidden claims without foreground return to hidden
/// under the same claim and identity: never re-admitted, never moved to the
/// wrong domain. Destroyed/recycled claims retire their ledger notes.
fn rehide_pass(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    let keys: Vec<crate::workspace::WindowKey> = state.hidden_claims.keys().cloned().collect();
    for key in keys {
        if key.hwnd == foreground {
            continue;
        }
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let visible = unsafe { IsWindowVisible(key.hwnd as isize as HWND) } != 0;
        if !visible {
            continue;
        }
        // Same-claim guard: only the exact stored identity returns to hidden.
        let snap = match crate::product_hide::sys::query_recovery(key.hwnd, me) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !crate::workspace_owner::member_matches(
            &key,
            key.hwnd,
            snap.pid,
            &snap.process.process_creation,
        ) || snap.process != record.claim.process
            || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
        {
            continue;
        }
        match crate::product_hide::sys::rehide_known_claim(me, &record.claim) {
            Ok(true) => {}
            Ok(false) => {
                // Retired: drop the ledger note through the identity-safe
                // reveal path and forget session membership safely.
                let _ = crate::product_hide::sys::reveal_product_claim_to(
                    store,
                    me,
                    &record.claim,
                    record.iconic,
                );
                state.hidden_claims.remove(&key);
            }
            Err(_) => {}
        }
    }
}
/// Sync monitor arrivals/removals with whole-workspace displacement/return:
/// displaced layouts are never merged, hidden windows never lost, and on
/// return the workspaces come back with then-current contents. Focus
/// preserves the surviving view unless the removed monitor held focus.
/// Mode-aware monitor topology sync (item 12 stage C): thin dispatcher into
/// [`scope_hotplug`]. Local keeps the legacy body below unchanged;
/// global-unique adds scoped history/trailing and shared skips displacement.
fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    scope_hotplug::sync_monitor_outputs(state, areas);
}

fn sync_monitor_outputs_local(state: &mut TileLoop, areas: &[MonitorArea]) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    // Returning displaced origins are recreated by `reconnect_output` without
    // baseline empties below: never `ensure_output` them here, or a duplicate
    // trailing/minimum pair appears beside the returning workspaces.
    let displaced_origins: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for key in &current {
        if displaced_origins.contains(key) {
            continue;
        }
        state.workspaces.ensure_output(key);
    }
    let gone: Vec<String> = state
        .known_outputs
        .iter()
        .filter(|k| !current.contains(k))
        .cloned()
        .collect();
    // Actual-foreground member view at disconnect time: when the removed
    // monitor held focus, the survivor shows that member's workspace instead
    // of an arbitrary one. Otherwise the surviving view keeps focus untouched.
    // The foreground key must also pass the visible lifetime gate, so a
    // same-process HWND reuse racing the disconnect never activates the wrong
    // workspace.
    let fg_workspace: Option<(String, String)> = {
        let fg = unsafe { GetForegroundWindow() } as usize as u64;
        let fg_tag = crate::product_hide::sys::read_member_tag(fg);
        state
            .member_tokens
            .iter()
            .find(|(k, _)| {
                k.hwnd == fg
                    && state.member_tags.get(*k).is_some_and(|stored| {
                        crate::workspace_owner::visible_lifetime_ok(stored, fg_tag.as_deref())
                    })
            })
            .and_then(|(k, _)| state.workspaces.member_loc(k).cloned())
            .map(|loc| (loc.output, loc.workspace))
    };
    let mut deferred: Vec<String> = Vec::new();
    for origin in gone {
        // Disconnect-time geometry decides the survivor: the origin's own
        // last-seen rectangle first, never post-disconnect frames as proxy;
        // then active output, then ordering. Never merge into a new split.
        let reference = state
            .last_areas
            .iter()
            .find(|a| a.device == origin)
            .or_else(|| state.last_areas.first())
            .map(|a| tiler_core::workspace::DisplacedRect {
                x: a.full.x,
                y: a.full.y,
                w: a.full.w,
                h: a.full.h,
            });
        let survivors: Vec<tiler_core::workspace::Survivor> = current
            .iter()
            .map(|k| {
                let rect = areas.iter().find(|a| &a.device == k).map(|a| {
                    tiler_core::workspace::DisplacedRect {
                        x: a.full.x,
                        y: a.full.y,
                        w: a.full.w,
                        h: a.full.h,
                    }
                });
                tiler_core::workspace::Survivor {
                    key: k.clone(),
                    rect,
                }
            })
            .collect();
        let active = if state.active_output.is_empty() {
            None
        } else {
            Some(state.active_output.as_str())
        };
        if let Some(dest) =
            tiler_core::workspace::choose_displaced_destination(&survivors, active, reference)
        {
            // Relocate retained Engine sessions first so layout trees,
            // contents, and focus move with the workspaces instead of
            // reseeding on the survivor. Empty or unusable sessions stay;
            // the outcome counts below stay opaque.
            let mut relocated = 0u32;
            let mut retained_count = 0u32;
            // Membership entries move below; Engine sessions follow per
            // workspace id through the existing relocation operation.
            let before: Vec<String> = state.workspaces.workspace_ids(&origin);
            for ws in &before {
                let target_key = tiler_core::session::DomainKey {
                    output: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                };
                let bounds = areas
                    .iter()
                    .find(|a| a.device == dest)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                let Some(bounds) = bounds else {
                    retained_count += 1;
                    continue;
                };
                let target_domain = tiler_core::session::OutputDomain {
                    id: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                    bounds,
                    gap: state.inner_gap,
                    adjacent: std::collections::BTreeMap::new(),
                };
                if state.engine.try_relocate_for_target(
                    &target_key,
                    &target_domain,
                    state.outer_gap,
                ) {
                    relocated += 1;
                } else {
                    retained_count += 1;
                }
            }
            state.workspaces.displace_output_to(&origin, &dest);
            if state.active_output == origin {
                state.active_output = dest.clone();
            }
            // The removed monitor held focus: show that member's workspace on
            // the survivor with its actual focus preserved. Any other case
            // keeps the survivor view with no stealing.
            if let Some((fg_output, fg_workspace)) = fg_workspace.clone()
                && fg_output == origin
            {
                state.workspaces.activate(&dest, &fg_workspace);
                if let Some(fg_key) = state
                    .member_tokens
                    .keys()
                    .find(|k| {
                        state
                            .workspaces
                            .member_loc(k)
                            .is_some_and(|l| l.output == dest && l.workspace == fg_workspace)
                    })
                    .cloned()
                {
                    state.workspaces.note_foreground(&fg_key);
                }
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-displace",
                    "output": state.workspaces.output_token(&dest),
                    "relocated": relocated,
                    "retained": retained_count,
                }),
            );
            // Hotplug-driven observed change records like any other: observe
            // the survivor's post-actuation view so a later toggle returns
            // to the actual preceding view. `displace_output_to` already
            // discarded the removed origin's history.
            if let Some(active) = state.workspaces.active_id(&dest) {
                state.workspaces.observe_workspace_change(&dest, &active);
            }
        } else {
            // No survivor: defer with known origins kept, never drop the
            // workspaces or their Engine sessions. The disconnected output's
            // history is still discarded with the output.
            state.workspaces.discard_output_history(&origin);
            deferred.push(origin);
        }
    }
    // Reconnect returns displaced workspaces to the exact origin key with
    // then-current contents; Engine sessions relocate back first so the
    // returning monitor shows then-current trees, not saved snapshots.
    // The active view shows the returning workspace only when it holds the
    // focused window, otherwise the surviving view keeps focus with no
    // stealing (best-effort: preserve active output).
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for origin in displaced {
        if current.contains(&origin) {
            let dest_key = state
                .workspaces
                .displaced_snapshot()
                .get(&origin)
                .map(|r| r.dest_key.clone());
            if let Some(record) = state.workspaces.displaced_snapshot().get(&origin).cloned() {
                let bounds = areas
                    .iter()
                    .find(|a| a.device == origin)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                if let Some(bounds) = bounds {
                    for ws in &record.workspace_ids {
                        let target_key = tiler_core::session::DomainKey {
                            output: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                        };
                        let target_domain = tiler_core::session::OutputDomain {
                            id: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                            bounds,
                            gap: state.inner_gap,
                            adjacent: std::collections::BTreeMap::new(),
                        };
                        state.engine.try_relocate_for_target(
                            &target_key,
                            &target_domain,
                            OUTER_GAP,
                        );
                    }
                }
            }
            state.workspaces.reconnect_output(&origin);
            // Reconnect primes a fresh baseline at the returning view and
            // never consults history for selection (see `reconnect_output`).
            // Record any subsequently observed survivor displacement here so
            // two changes in one handler leave the actual immediately
            // preceding view: the survivor's post-return view observes
            // against its pre-return baseline.
            let dest_key = dest_key.unwrap_or_else(|| state.active_output.clone());
            if !dest_key.is_empty()
                && let Some(active) = state.workspaces.active_id(&dest_key)
            {
                state
                    .workspaces
                    .observe_workspace_change(&dest_key, &active);
            }
        }
    }
    // Deferred no-survivor origins stay known with their last geometry until
    // a survivor exists; their workspaces and Engine sessions are untouched.
    let mut known = current.clone();
    for origin in &deferred {
        if !known.contains(origin) {
            known.push(origin.clone());
        }
    }
    state.known_outputs = known;
    let mut last = areas.to_vec();
    for origin in &deferred {
        if !last.iter().any(|a| &a.device == origin)
            && let Some(area) = state
                .last_areas
                .iter()
                .find(|a| &a.device == origin)
                .cloned()
        {
            last.push(area);
        }
    }
    state.last_areas = last;
    if state.active_output.is_empty()
        && let Some(first) = state.known_outputs.first().cloned()
    {
        state.active_output = first;
    }
}

/// Per-tick workspace maintenance on quiet ticks: monitor sync, new-window
/// assignment, close cleanup, foreground-driven select, and same-claim
/// rehide of externally revealed hidden windows.
fn workspace_maintenance(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    sync_monitor_outputs(state, areas);
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Unconfirmed toggle releases retry here on the maintenance observation
    // (no second enumeration): the fingerprint gate inside fires only on a
    // real topology edge, so quiet ticks never poll or log.
    retry_pending_releases(state, me, fulls, areas, &observed, &retained);
    // Periodic hidden-claim retirement before any foreground-driven switch so
    // dead claims never select.
    audit_hidden_claims(state, me, store);
    // Foreground-driven select only on an actual foreground change to an
    // exactly verified hidden member; event-only wakes never switch.
    if poll_foreground_workspace(state, me, store, dir, fulls, areas, &mut observed).is_some() {
        return;
    }
    rehide_pass(state, me, store);
}

/// END-bound Esc latch decision: a physical Esc down cancels the gesture
/// only when the observed sequence differs from the START snapshot. Held
/// gestures observe the live counter; just-ended gestures observe the
/// END-callback snapshot, so an Esc that lands after the END (before the
/// owner drain) observes an equal snapshot and never cancels the completed
/// drop. Pure sequence comparison (wrapping-aware by construction: the
/// counter only moves forward one step per physical Esc down).
#[must_use]
fn esc_edge_cancels(start_seq: u64, observed_seq: u64) -> bool {
    observed_seq != start_seq
}

/// Shared START capture for both gesture producers (native WinEvent and
/// project Win+Left): the Esc-sequence snapshot, the PRE-gesture rectangle,
/// and the START-time identity snapshot. PRE comes from the last stable
/// observation, never from a mid-gesture frame; live `GetWindowRect` is the
/// fallback only when `stable` has no entry. Probe failure leaves no
/// snapshot and settle falls back to the stored-member check only.
fn capture_gesture_start(state: &mut TileLoop, me: &ProcessIdentity, hwnd: u64, start_seq: u64) {
    // Sequence-bound Esc coordination: the snapshot rode with the START
    // event from callback time instead of being re-read here. Only edges
    // with a newer sequence latch later, so a stale pre-gesture tap never
    // cancels and a same-batch edge is ordered rather than discarded.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_esc_seq.entry(hwnd) {
        e.insert(start_seq);
    }
    if !state.gesture_before.contains_key(&hwnd) {
        let pre = state
            .stable
            .get(&hwnd)
            .copied()
            .or_else(|| live_outer_rect(hwnd).or_else(|| state.stable.get(&hwnd).copied()));
        if let Some(pre) = pre {
            state.gesture_before.insert(hwnd, pre);
        }
    }
    // START-time identity snapshot for the settle-time reuse fence: a
    // same-process HWND reuse between START and END fails closed even when
    // HWND/PID/creation still agree.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_start_key.entry(hwnd) {
        let probe = hwnd as isize as HWND;
        if let Some(live) = window_identity(probe, hwnd, me) {
            e.insert(crate::workspace::WindowKey {
                hwnd,
                pid: live.pid,
                creation: live.process_creation.clone(),
            });
            // Member lifetime tag, not the owned-helper tag (empty for
            // ordinary windows): the settle fence compares this against a
            // fresh member read, exactly like the admission gate.
            state
                .gesture_start_tag
                .insert(hwnd, crate::product_hide::sys::read_member_tag(hwnd));
        }
    }
    // START-frozen preview source binding (first START wins, never
    // rebound): the exact member token, source output/workspace, and
    // accepted revision the preview samples and the final drop resolve
    // against. Unmanaged STARTs store nothing and fail closed in the
    // refresh; drift after START fails the whole gesture closed.
    if !state.gesture_preview_start.contains_key(&hwnd) {
        let start_binding = state
            .member_tokens
            .iter()
            .find(|(key, _)| key.hwnd == hwnd)
            .and_then(|(key, token)| {
                state.workspaces.member_loc(key).cloned().map(|loc| {
                    let revision = revision_for(state, &loc.output, &loc.workspace);
                    PreviewStartBinding {
                        token: token.clone(),
                        output: loc.output,
                        workspace: loc.workspace,
                        revision,
                    }
                })
            });
        match start_binding {
            Some(binding) => {
                state.gesture_preview_start.insert(hwnd, binding);
            }
            None => {
                state.gesture_preview_start.remove(&hwnd);
            }
        }
    }
}

/// One drained project Win+Left down edge: validate the callback-bound
/// candidate against fresh evidence BEFORE any effect or focus, then arm
/// the shared gesture maps so the hold pauses tiling and the release
/// settles through the shared route. After all START fences pass and the
/// arm captures `active`, the mover is focused at press through the
/// existing [`actuate_focus`] authority (R-DRAG-08): a suspend/elevation
/// precheck refuses first (no prime against a known vetoing arrival), then
/// a fresh exact foreground readback avoids a redundant setter when the
/// mover already holds foreground, otherwise one `actuate_focus` attempt
/// decides (its late fence stays the second race guard). A refused press
/// clears only this HWND's arm with no Engine plan and no geometry change;
/// a failed actuation may still have attempted the E8 prime/setter, so only
/// geometry/plan silence is claimed, never zero focus writes. No geometry
/// and no Engine call here. Returns the bound token when the gesture armed.
///
/// The callback snapshot (published origin plus member tag cloned at Down
/// time) must still match the current published entry AND a fresh live
/// probe on every field, closing the callback-to-drain HWND-reuse gap: a
/// different-process reuse fails PID/creation, a same-process reuse fails
/// the lifetime tag, and a retokened member fails the token. The validated
/// snapshot is stored as the bound generation; the Up requires it and never
/// rebinds.
///
/// A refused Down carries no identity: only the closed refusal reason
/// enters the log, never HWNDs or tokens.
fn windrag_down(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    edge: &WinDragEdge,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> std::result::Result<String, &'static str> {
    let hwnd = edge.hwnd;
    let Some(snapshot) = edge.snapshot.as_ref() else {
        return Err("no-snapshot");
    };
    // Tiled-only published origins plus current management: floating,
    // sticky, unmanaged, and unknown subjects never arm (their Win+Left
    // passed through natively hook-side).
    if !state.managed.contains(&hwnd) {
        return Err("unmanaged");
    }
    let Some(published) = state.windrag_origins.get(&hwnd) else {
        return Err("unknown-subject");
    };
    if state.active.contains(&hwnd)
        || state.gesture_before.contains_key(&hwnd)
        || state.windrag_bound.contains_key(&hwnd)
    {
        return Err("busy");
    }
    // Fresh live evidence on the owner thread: the snapshot must still
    // match the live window, not just the last publish.
    let probe = hwnd as isize as HWND;
    let Some(live) = window_identity(probe, hwnd, me) else {
        return Err("identity-changed");
    };
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    if !validate_windrag_down(
        snapshot,
        &published.origin,
        &published.tag,
        live.pid,
        &live.process_creation,
        &live_tag,
    ) {
        return Err("stale-snapshot");
    }
    // The snapshot token must still name this exact live member.
    let live_key = crate::workspace::WindowKey {
        hwnd,
        pid: live.pid,
        creation: live.process_creation.clone(),
    };
    if state
        .member_tokens
        .get(&live_key)
        .is_none_or(|token| token != &snapshot.origin.token)
    {
        return Err("no-token");
    }
    capture_gesture_start(state, me, hwnd, edge.esc_seq);
    if !state.gesture_before.contains_key(&hwnd) {
        // No PRE frame (not stable and unreadable live): fail closed with
        // no arm rather than settling without a source allocation.
        state.gesture_esc_seq.remove(&hwnd);
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        return Err("no-pre");
    }
    state.gesture_producer.insert(hwnd, "windrag");
    state.windrag_start_cursor.insert(hwnd, (edge.x, edge.y));
    state.windrag_bound.insert(hwnd, snapshot.clone());
    // A/B move arm: the project hold classifies as a move, so a focused
    // Win+Left hold reveals the projected group underlay while held (no
    // stage C, no unfocused Engine focus substitution). Cleaned on
    // Up/cancel/suspend/settle like every other gesture map.
    state.move_kind.insert(hwnd, MoveSizeKind::Move);
    state.active.insert(hwnd);
    // R-DRAG-08 press focus: the validated mover activates at press through
    // the same authority as the drop path, after every START fence and the
    // `active` capture above. A suspend/elevation precheck refuses first so
    // a known vetoing fullscreen arrival (or an elevated foreground) is
    // never primed against; the late fence inside `actuate_focus` stays the
    // second guard for arrivals racing this drain. Failure refuses the whole
    // arm via the scoped clear (this HWND's maps only: a concurrently armed
    // neighbour's preview is never disturbed) with no Engine plan and no
    // geometry change. A failed actuation may still have attempted the E8
    // prime/setter, so only geometry/plan silence is claimed.
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err("focus-skipped-fence");
    }
    let token = snapshot.origin.token.clone();
    if unsafe { GetForegroundWindow() } as usize as u64 == hwnd {
        // Already exact foreground: no prime, no setter, no wait.
        let log_path = state.log_path.clone();
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "windrag-focus",
                "tick": tick,
                "phase": "press",
                "outcome": "focus-ok",
                "already_foreground": true,
                "prime_inserted": 0,
                "setter_accepted": false,
                "attach_ok": false,
                "eventual": false,
                "duration_ms": 0,
                "window": token.as_str(),
                "producer": "windrag",
            }),
        );
        return Ok(token);
    }
    let focus_start = Instant::now();
    let actuation = actuate_focus(state, me, fulls, observed, retained, &token);
    let duration_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if actuation.outcome != "focus-ok" {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err(actuation.outcome);
    }
    let log_path = state.log_path.clone();
    let tick = state.tick;
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "windrag-focus",
            "tick": tick,
            "phase": "press",
            "outcome": actuation.outcome,
            "already_foreground": false,
            "prime_inserted": actuation.prime_inserted,
            "setter_accepted": actuation.setter_accepted,
            "attach_ok": actuation.attach_ok,
            "eventual": actuation.eventual,
            "duration_ms": duration_ms,
            "window": token.as_str(),
            "producer": "windrag",
        }),
    );
    Ok(token)
}

/// One drained project Win+Left up edge: carry the callback-bound release
/// pointer to settle and close the hold. Requires the bound generation
/// stored at arm time, so a stray or replayed Up can never rebind or
/// settle a gesture it did not close. The shared `ended` computation below
/// settles it through `gesture_tick` on this same pump.
fn windrag_up(state: &mut TileLoop, hwnd: u64, x: i32, y: i32, esc_seq: u64) {
    if !state.windrag_bound.contains_key(&hwnd) {
        return;
    }
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag") {
        return;
    }
    if !state.active.contains(&hwnd) {
        return;
    }
    state.gesture_end_cursor.insert(hwnd, (x, y));
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_end_seq.entry(hwnd) {
        e.insert(esc_seq);
    }
    state.active.remove(&hwnd);
    state.move_kind.remove(&hwnd);
}

/// Clear every project-gesture map for one HWND (owner-side Cancel): the
/// hook already disarmed and queued this edge, so the paired Up stays
/// swallowed hook-side while the owner drops the gesture with no plan and
/// no geometry change. Native gestures on the same HWND are untouched:
/// only entries produced by a validated windrag arm are removed.
/// `hide_visible_fallback` keeps the legacy Cancel behavior (a visible
/// singleton overlay hides with the gesture even without an owned binding);
/// the press-failure path passes `false` so a concurrently armed
/// neighbour's preview is never disturbed (at press this HWND owns no
/// preview binding yet).
fn clear_windrag_gesture(state: &mut TileLoop, hwnd: u64) -> Option<String> {
    clear_windrag_gesture_scoped(state, hwnd, true)
}

/// Scoped variant of [`clear_windrag_gesture`]; see `hide_visible_fallback`.
fn clear_windrag_gesture_scoped(
    state: &mut TileLoop,
    hwnd: u64,
    hide_visible_fallback: bool,
) -> Option<String> {
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag")
        && !state.windrag_bound.contains_key(&hwnd)
    {
        return None;
    }
    let token = state
        .windrag_bound
        .get(&hwnd)
        .map(|bound| bound.origin.token.clone());
    let had_preview = state.preview_bound.remove(&hwnd).is_some();
    state.preview_dead.remove(&hwnd);
    state.gesture_preview_start.remove(&hwnd);
    state.gesture_start_cursor.remove(&hwnd);
    // Owner-side cancel clears the preview with the gesture (the hook
    // already disarmed; the paired Up stays swallowed hook-side).
    if had_preview
        || (hide_visible_fallback && render_carrier_visible(state, RenderCarrier::Preview))
    {
        hide_preview(state, "cancelled");
    }
    state.active.remove(&hwnd);
    state.gesture_before.remove(&hwnd);
    state.gesture_end_cursor.remove(&hwnd);
    state.gesture_start_key.remove(&hwnd);
    state.gesture_start_tag.remove(&hwnd);
    state.gesture_esc_seq.remove(&hwnd);
    state.gesture_end_seq.remove(&hwnd);
    state.esc_latched.remove(&hwnd);
    state.gesture_producer.remove(&hwnd);
    state.windrag_start_cursor.remove(&hwnd);
    state.windrag_bound.remove(&hwnd);
    state.move_kind.remove(&hwnd);
    // Owner-side cancel also drops a queued bridged finish for the same
    // HWND: the gesture is gone, so a late hide ack must never actuate it.
    // No Engine/focus/geometry here; the service logs the refusal when it
    // observes the missing slot, or this drops it immediately when the
    // bridge is present.
    if state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .is_some_and(|pending| pending.hwnd == hwnd)
    {
        if let Some(bridge) = state.render_bridge.as_mut() {
            bridge.pending.take();
        }
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
    }
    token
}

/// Pending invalidation while a bridged finish waits (or after its hide
/// proves): stop, Esc/cancel, suspend/elevated, or a new gesture for the
/// same HWND. Returns the narrow refusal vocabulary when invalidated, else
/// `None`. Pure decision over owner state plus the suspend/elevated reads
/// (injected in tests, native in production); no Engine, focus, geometry,
/// or pending mutation here. The caller drops the slot.
fn pending_invalidation(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    hwnd: u64,
) -> Option<(&'static str, &'static str)> {
    #[cfg(test)]
    if INJECTED_STOP.with(|slot| slot.get()) {
        return Some(("drag-drop-refused", "preview-finish-stopped"));
    }
    if state.esc_latched.contains(&hwnd) {
        return Some(("gesture-cancelled-esc", "esc"));
    }
    if state.suspended {
        return Some(("drag-drop-refused", "preview-finish-suspended"));
    }
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        return Some(("drag-drop-refused", "preview-finish-suspended"));
    }
    // A new gesture for the same HWND supersedes the queued finish: the
    // pending slot owns its START snapshots, but a fresh `active` capture
    // means the window started moving again after END. Fail the stale slot
    // closed; the new gesture settles on its own END.
    if state.active.contains(&hwnd) {
        return Some(("drag-drop-refused", "preview-finish-superseded"));
    }
    None
}

/// Queue one bridged finish as a bounded pending owner operation (END path).
/// Hides the preview (the safety epoch bars any stale-show replay) and
/// stashes the drop with its hide pair and deadline. Returns `true` when a
/// previous finish still owns the slot (overlapping END race): the newcomer
/// is refused narrowly with no drop and no reconcile, and the pending
/// operation owns the settle. Single slot: one gesture settles at a time.
/// Production END queueing funnels through here; tests exercise it directly
/// with offline fixtures (channels only, no renderer thread).
#[allow(clippy::too_many_arguments)]
fn queue_pending_drop(
    state: &mut TileLoop,
    hwnd: &u64,
    producer: &'static str,
    intent: GestureIntent,
    mover_token: &str,
    start_key: Option<crate::workspace::WindowKey>,
    start_tag: Option<Option<String>>,
    member_key: crate::workspace::WindowKey,
    output: &str,
    workspace: &str,
    dest: Option<PendingCrossDest>,
    source_rect: Option<Rect>,
) -> bool {
    let drop_prior = state
        .preview_bound
        .get(hwnd)
        .and_then(|bound| bound.hover_prior.clone());
    hide_render_carrier(state, RenderCarrier::Preview, "finish");
    state
        .render_bridge
        .as_mut()
        .map(|bridge| {
            let index = RenderCarrier::Preview.index();
            let pair = (
                bridge.gens[index],
                bridge.channels.hide_mark(RenderCarrier::Preview),
            );
            if bridge.pending.is_some() {
                true
            } else {
                bridge.pending = Some(PendingDrop {
                    hwnd: *hwnd,
                    producer: producer.to_owned(),
                    intent,
                    mover_token: mover_token.to_owned(),
                    start_key,
                    start_tag,
                    member_key,
                    output: output.to_owned(),
                    workspace: workspace.to_owned(),
                    drop_prior,
                    dest,
                    source_rect,
                    hide_gen: pair.0,
                    hide_safety: pair.1,
                    deadline: Instant::now() + FINISH_WAIT,
                });
                false
            }
        })
        .unwrap_or(false)
}

/// Service one proven-hide cross-output pending drop: re-resolve the pointer
/// destination, re-verify the END-time fences against fresh state, then run
/// the shared Engine route. No pre-drop focus: the confirm tail
/// follow-focuses on the destination after the transfer, so a cross drop
/// never foregrounds the source first. Every refusal converges without
/// Engine, focus, or geometry writes.
#[allow(clippy::too_many_arguments)]
fn service_cross_pending_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    current: &ObservedWindow,
    member_key: &crate::workspace::WindowKey,
    loc: &crate::workspace::MemberLoc,
    pending: &PendingDrop,
    dest: &PendingCrossDest,
    x: i32,
    y: i32,
) {
    let log_path = state.log_path.clone();
    let from = current.token.clone();
    let tick = state.tick;
    let log_cross = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": outcome,
                "window": from,
                "producer": pending.producer,
                "cross_output": true,
            }),
        );
    };
    // The pointer output must still resolve to the queued destination:
    // topology moved under the pending finish otherwise.
    if output_for_point(areas, x, y).as_deref() != Some(dest.output.as_str()) {
        log_cross("topology-changed");
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == loc.output && fresh.workspace == loc.workspace);
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
    let retained_full = retained
        .iter()
        .find(|r| r.key == *member_key)
        .map(|r| r.fullscreen)
        .unwrap_or(false);
    let live = drag_drop_live(
        state,
        areas,
        &dest.fence,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(retained_full),
    );
    if let Some(outcome) = crate::workspace_owner::drag_drop_fences_hold(&dest.fence, &live) {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_cross(outcome);
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    // Fresh suspension fence after the proven hide, before any Engine or
    // project mutation: a fullscreen arrival (or elevated foreground)
    // between END and the ack refuses with no plan and no writes.
    if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        log_cross(if state.suspended {
            "suspended"
        } else {
            "elevated-foreground"
        });
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = CrossDragSettle {
        mover_token: current.token.clone(),
        member_key: member_key.clone(),
        source_output: loc.output.clone(),
        source_workspace: loc.workspace.clone(),
        dest_output: dest.output.clone(),
        dest_workspace: dest.workspace.clone(),
        x,
        y,
        drop_prior: pending.drop_prior.clone(),
        producer: pending.producer.clone(),
        start_rect: pending.source_rect,
    };
    settle_cross_drag_drop(
        state,
        me,
        fulls,
        areas,
        observed,
        retained,
        &settle,
        &dest.fence,
        tick,
        &correlation,
        correlation.as_str(),
    );
}

/// Service the bounded pending gesture drop from render-ack state at loop
/// level (ack-only and full turns alike): no blocking wait, so the owner
/// stays responsive to stop and new events. On a proven `Hidden`
/// acknowledgement the settle re-enters below with fresh observations and
/// gates before every focus/Engine/foreign write. On timeout the drop is
/// refused narrowly with no drop and no reconcile: actuation runs only over
/// a proven hide, while the posted safety epoch already bars any stale-show
/// replay. Cancel/Esc/suspend/new-gesture/stop invalidate the pending slot
/// with no Engine/focus/geometry, so a late ack can never actuate after the
/// gesture was cancelled, suspended, superseded, or stopped.
fn service_pending_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let Some(bridge) = state.render_bridge.as_ref() else {
        return;
    };
    let pending = match bridge.pending.as_ref() {
        Some(pending) => pending,
        None => return,
    };
    let (hwnd, hide_gen, hide_safety, deadline) = (
        pending.hwnd,
        pending.hide_gen,
        pending.hide_safety,
        pending.deadline,
    );
    // Invalidation while waiting (unproven or proven): stop, Esc/cancel,
    // suspend, or a new gesture for the same HWND drops the slot with no
    // Engine/focus/geometry. Checked before the timeout so a cancelled
    // pending never waits out the bound.
    if let Some((outcome, kind)) = pending_invalidation(state, me, fulls, hwnd) {
        let mover_token = state
            .render_bridge
            .as_ref()
            .and_then(|bridge| bridge.pending.as_ref())
            .map(|pending| pending.mover_token.clone())
            .unwrap_or_default();
        if let Some(bridge) = state.render_bridge.as_mut() {
            bridge.pending.take();
        }
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
        // Esc invalidation converges like the inline Esc path (one bounded
        // reconcile to snap back); stop/suspend/supersede drop without
        // converging, since the loop is stopping/suspended or the new
        // gesture owns the settle.
        let reconcile = outcome == "gesture-cancelled-esc";
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": outcome,
                "kind": kind,
                "window": mover_token,
                "producer": "settle",
            }),
        );
        if reconcile {
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
        }
        return;
    }
    let proven = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.hidden_ack[RenderCarrier::Preview.index()])
        .is_some_and(|(hgen, safe)| hgen >= hide_gen && safe >= hide_safety);
    if !proven {
        if Instant::now() >= deadline {
            // Timeout: refuse narrowly. No drop, no reconcile: the drop and
            // its convergence run only over a proven hide, and the safety
            // epoch already bars replay. The renderer hides on its next
            // quantum either way.
            let mover_token = state
                .render_bridge
                .as_ref()
                .and_then(|bridge| bridge.pending.as_ref())
                .map(|pending| pending.mover_token.clone())
                .unwrap_or_default();
            if let Some(bridge) = state.render_bridge.as_mut() {
                bridge.pending.take();
            }
            state.gesture_start_key.remove(&hwnd);
            state.gesture_start_tag.remove(&hwnd);
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "drag-drop",
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": "preview-finish-unacked",
                    "window": mover_token,
                    "producer": "settle",
                }),
            );
        }
        return;
    }
    let Some(pending) = state
        .render_bridge
        .as_mut()
        .and_then(|bridge| bridge.pending.take())
    else {
        return;
    };
    state.gesture_start_key.remove(&pending.hwnd);
    state.gesture_start_tag.remove(&pending.hwnd);
    let log_path = state.log_path.clone();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":state.tick,"cause":"enum-failed"}),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    // Fresh revalidation against the stashed START snapshots before any
    // write: a reuse between END and the proven hide fails closed here
    // exactly like the inline settle path.
    let Some(current) = by_hwnd.get(&pending.hwnd) else {
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    };
    if let (Some(key), Some(stored)) = (pending.start_key.as_ref(), pending.start_tag.as_ref())
        && (!crate::workspace_owner::member_matches(
            key,
            current.hwnd,
            current.identity.pid,
            &current.identity.process_creation,
        ) || {
            let live = live_member_tag(current.hwnd);
            stored != &live
        })
    {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "refused",
                "outcome": "gesture-refused-identity",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == current.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        return;
    };
    // The stashed END-time membership must still name the same member:
    // a token remap between END and the proven hide fails closed instead
    // of dropping under a recycled identity.
    if member_key != pending.member_key {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "refused",
                "outcome": "gesture-refused-identity",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    if !crate::workspace_owner::member_matches(
        &member_key,
        current.hwnd,
        current.identity.pid,
        &current.identity.process_creation,
    ) {
        return;
    }
    if state.sticky.contains_key(&member_key) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-refused-sticky",
                "window": current.token,
            }),
        );
        return;
    }
    if let Some(loc) = state.workspaces.member_loc(&member_key).cloned()
        && (engine_is_float(state, &loc.output, &loc.workspace, &current.token)
            || !workspace_mode_tiled(state, &loc.output, &loc.workspace))
    {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-refused-floating",
                "window": current.token,
            }),
        );
        return;
    }
    let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
        return;
    };
    if loc.output != pending.output || loc.workspace != pending.workspace {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "refused",
                "outcome": "gesture-refused-domain",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        return;
    };
    if let GestureIntent::MoveDrop { x, y } = pending.intent {
        match &pending.dest {
            None => {
                if !drop_point_in_domain(
                    domain.bounds.x,
                    domain.bounds.y,
                    domain.bounds.w,
                    domain.bounds.h,
                    x,
                    y,
                ) {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "drag-drop",
                            "disposition": "refused",
                            "outcome": "drag-drop-refused",
                            "kind": "outside-work-area",
                            "window": current.token,
                            "producer": pending.producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    return;
                }
            }
            Some(dest) => {
                // Cross-output pending settle: the hide already proved, so
                // revalidate everything fresh (destination resolution,
                // fences, suspend/elevated) then run the shared Engine
                // route. No pre-drop focus here: the confirm tail
                // follow-focuses on the destination after the transfer.
                service_cross_pending_drop(
                    state,
                    me,
                    fulls,
                    areas,
                    &observed,
                    &retained,
                    current,
                    &member_key,
                    &loc,
                    &pending,
                    dest,
                    x,
                    y,
                );
                return;
            }
        }
    }
    if pending.producer == "windrag" {
        // Fresh suspension fence after the proven hide, before any focus
        // write: a fullscreen arrival (or elevated foreground) between END
        // and the ack refuses with no focus and no Engine plan. The late
        // fence inside `actuate_focus` stays the second guard for arrivals
        // racing this re-entry.
        if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-focus",
                    "window": current.token,
                    "producer": pending.producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            return;
        }
        let actuation = actuate_focus(state, me, fulls, &observed, &retained, &current.token);
        if actuation.outcome != "focus-ok" {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-focus",
                    "window": current.token,
                    "producer": pending.producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            return;
        }
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "windrag-focus",
                "tick": state.tick,
                "phase": "drop",
                "outcome": "focus-ok",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
    }
    state.tick += 1;
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        &observed,
        &retained,
        "gesture",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        return;
    };
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let mover = WindowId(current.token.clone());
    {
        let revision = revision_for(state, &loc.output, &loc.workspace);
        let outer_gap = state.outer_gap;
        adopt_gaps_for_route(
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            Some(&mover),
            revision,
            fp,
            &correlation,
        );
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        Some(&mover),
    );
    let drop_prior_desc = preview_prior_desc(&pending.drop_prior);
    let reply = match pending.intent {
        GestureIntent::MoveDrop { x, y } => {
            event.command = CoreCommand::DragDrop {
                window: current.token.clone(),
                x,
                y,
                hover_prior: pending.drop_prior,
                source: None,
            };
            handle_engine_event(state, &mut event)
        }
        GestureIntent::PointerResize {
            direction,
            boundary,
            direction2,
            boundary2,
        } => {
            event.command = CoreCommand::PointerResize {
                window: current.token.clone(),
                direction: direction.to_owned(),
                boundary,
                direction2: direction2.map(str::to_owned),
                boundary2,
            };
            handle_engine_event(state, &mut event)
        }
    };
    let op = match pending.intent {
        GestureIntent::MoveDrop { .. } => "drag-drop",
        GestureIntent::PointerResize { .. } => "pointer-resize",
    };
    match &reply {
        CoreReply::Tiled(_) | CoreReply::Projection(_) | CoreReply::Resize(_) => {
            let tick = state.tick;
            let fulls_owned = fulls.to_vec();
            let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
            apply_geometry(
                state,
                ApplyInput {
                    me,
                    fulls: &fulls_owned,
                    reply: &reply,
                    observed: &observed,
                    op,
                    tick,
                    correlation: correlation.as_str(),
                    skipped: Vec::new(),
                    writable: &writable,
                    output_token: state.workspaces.output_token(&loc.output),
                    workspace_token: state
                        .workspaces
                        .workspace_token(&loc.output, &loc.workspace),
                    revision: revision_for(state, &loc.output, &loc.workspace),
                },
            );
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "applied",
                    "outcome": if op == "drag-drop" { "drag-drop-applied" } else { "pointer-resize-applied" },
                    "window": current.token,
                    "producer": pending.producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
        }
        _ => {
            let kind = match &reply {
                CoreReply::Rejected { kind, .. } => *kind,
                CoreReply::Diverged(reason) => reason.as_str(),
                CoreReply::SnapshotInvalid { detail, .. } => *detail,
                _ => "unhandled-variant",
            };
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": kind,
                    "window": current.token,
                    "producer": pending.producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
        }
    }
}

fn gesture_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    ended: &[u64],
) {
    let log_path = state.log_path.clone();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":state.tick,"cause":"enum-failed"}),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    for hwnd in ended {
        // Settle producer travels with the gesture: `native` for the
        // title-bar modal loop, `windrag` for the project-driven Win+Left
        // stationary hold. Absent entries settle as native.
        let producer = state.gesture_producer.remove(hwnd).unwrap_or("native");
        // Per-gesture START snapshot and END cursor travel with the gesture:
        // remove them here so each END settles exactly once.
        let start_key = state.gesture_start_key.remove(hwnd);
        let start_tag = state.gesture_start_tag.remove(hwnd);
        let end_cursor = state.gesture_end_cursor.remove(hwnd);
        let start_cursor = state.windrag_start_cursor.remove(hwnd);
        // Modal move/size truth sampled once at START: read and clear here
        // so every END path (settle, cancel, refusal) leaves no stale arm.
        // The native branch below prefers it over rect comparison.
        let modal_kind = state.move_kind.remove(hwnd);
        state.windrag_bound.remove(hwnd);
        state.gesture_esc_seq.remove(hwnd);
        state.gesture_end_seq.remove(hwnd);
        // Explicit Esc cancellation first: a cancelled native move restores
        // its pre-gesture frame, so rect equality must never decide this
        // path. Snap back through one bounded reconcile with no Engine
        // mutation here at all; topology unchanged by construction.
        if state.esc_latched.remove(hwnd) {
            let token = by_hwnd.get(hwnd).map(|w| w.token.clone()).or_else(|| {
                state
                    .member_tokens
                    .iter()
                    .find(|(key, _)| key.hwnd == *hwnd)
                    .map(|(_, token)| token.clone())
            });
            let mut line = serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-cancelled-esc",
                "producer": producer,
            });
            if let Some(token) = token {
                line["window"] = serde_json::Value::from(token);
            }
            log_json_at(&log_path, line);
            // Esc cancel clears the preview with the gesture (already
            // hidden at latch time; this covers latch-and-settle races).
            hide_preview(state, "esc-cancelled");
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        let Some(current) = by_hwnd.get(hwnd) else {
            // A maximized/fullscreen member is retained, never eligible, so a
            // gesture ending on one misses the observation above. Refuse
            // explicitly with no Engine mutation; the next ordinary tick
            // converges. Identity-fenced on the stored member identity.
            if let Some(key) = state
                .member_tokens
                .iter()
                .find(|(k, _)| k.hwnd == *hwnd)
                .map(|(k, _)| k.clone())
                && let Some(stored) = state.member_identity.get(&key).cloned()
                && crate::workspace_owner::member_matches(
                    &key,
                    *hwnd,
                    stored.pid,
                    &stored.process_creation,
                )
                && let Some(row) = retained.iter().find(|r| r.key == key)
                && let Some(cause) = overlay_refusal_for(row)
                && let Some(token) = state.member_tokens.get(&key).cloned()
            {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": if cause == "fullscreen" {
                            "gesture-refused-fullscreen"
                        } else {
                            "gesture-refused-maximize"
                        },
                        "window": token,
                    }),
                );
            }
            continue;
        };
        let before = state
            .gesture_before
            .get(hwnd)
            .copied()
            .or_else(|| state.stable.get(hwnd).copied());
        let Some(before) = before else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-no-start",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        };
        // START/END reuse fence: the fresh window must still match the
        // START-time snapshot when one was captured. A same-process HWND
        // reuse between START and END keeps HWND/PID/creation but starts
        // without (or with a stale) member lifetime tag, so it fails closed
        // here exactly like every other effect gate. No snapshot falls back
        // to the stored-member check below. The tag comparison is snapshot
        // versus a fresh member-property read (never the owned-helper tag,
        // which is empty for ordinary windows).
        if let (Some(key), Some(stored)) = (start_key.as_ref(), start_tag.as_ref())
            && (!crate::workspace_owner::member_matches(
                key,
                current.hwnd,
                current.identity.pid,
                &current.identity.process_creation,
            ) || {
                let live = live_member_tag(current.hwnd);
                stored != &live
            })
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        // Release cursor carried from the MOVESIZEEND event callback, never
        // re-read later: an END without a captured cursor routes as no-change
        // for moves (cursor-less), while resizes classify from geometry
        // alone. The project Win+Left hold bypasses rect classification
        // entirely: its frames stay at source by design, so the tracked
        // POINTER journey (down point versus callback-bound release point)
        // decides, through the same Engine drop route below.
        let intent = if producer == "windrag" {
            let (Some(start), Some(end)) = (start_cursor, end_cursor) else {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": "gesture-no-change",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            match settle_pointer_journey(start.0, start.1, end.0, end.1, false) {
                WinSettle::Cancelled | WinSettle::NoChange => {
                    // Esc reaches here only via the latch above; a zero
                    // pointer journey makes no plan and changes no geometry.
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "gesture",
                            "disposition": "observed",
                            "outcome": "gesture-no-change",
                            "window": current.token,
                            "producer": producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    continue;
                }
                WinSettle::Drop { x, y } => GestureIntent::MoveDrop { x, y },
            }
        } else {
            let cursor = end_cursor;
            let Some(mut classified) = classify_gesture(&before, &current.visible, cursor) else {
                // Zero movement (or a cursor-less move): nothing to route.
                // Reconcile once so any native nudge is reasserted to the
                // Engine allocation; topology unchanged.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": "gesture-no-change",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            // Modal truth over rect comparison (see
            // `tiling::prefer_modal_move`): a caption-started move loop
            // carries user move intent even through a system rescale.
            classified = crate::tiling::prefer_modal_move(
                classified,
                cursor,
                modal_kind.is_some_and(|kind| kind == MoveSizeKind::Move),
            );
            classified
        };
        // The gesture window's own (output, workspace) session owns this
        // settle: same per-workspace routing as directional chords.
        let member_key = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == current.token.as_str())
            .map(|(key, _)| key.clone());
        let Some(member_key) = member_key else {
            continue;
        };
        if !crate::workspace_owner::member_matches(
            &member_key,
            current.hwnd,
            current.identity.pid,
            &current.identity.process_creation,
        ) {
            continue;
        }
        // Floats never feed gesture intents to the Engine; native float
        // move/resize stays free. Sticky rides the same slotless float plus
        // an explicit gate so a pruned-domain sticky still refuses.
        if state.sticky.contains_key(&member_key) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-refused-sticky",
                    "window": current.token,
                }),
            );
            continue;
        }
        if let Some(loc) = state.workspaces.member_loc(&member_key).cloned()
            && (engine_is_float(state, &loc.output, &loc.workspace, &current.token)
                || !workspace_mode_tiled(state, &loc.output, &loc.workspace))
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-refused-floating",
                    "window": current.token,
                }),
            );
            continue;
        }
        let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
            continue;
        };
        let Some((domain, domain_key)) = workspace_domain_for(
            &loc.output,
            &loc.workspace,
            areas,
            state.inner_gap,
            state.outer_gap,
        ) else {
            continue;
        };
        // Cross-output drops (REQ-MOU-03): the pointer output's current
        // view scopes the drop through the pair route (occupied) or the
        // singleton admission (empty). Same-output points keep the local
        // route below; unplaceable points refuse with snap-back and no
        // transfer. The release cursor is END-captured (never re-read), and
        // domain decisions use retained membership plus this point, never
        // the possibly-moved native frame monitor.
        let cross_dest = if let GestureIntent::MoveDrop { x, y } = intent {
            match resolve_drop_destination(state, areas, &loc.output, x, y) {
                DropDestination::SameOutput => {
                    // Same-output points still sit inside the inset work
                    // area: panel-strip releases refuse before any Engine
                    // call, exactly like the retired same-output fence.
                    if !drop_point_in_domain(
                        domain.bounds.x,
                        domain.bounds.y,
                        domain.bounds.w,
                        domain.bounds.h,
                        x,
                        y,
                    ) {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "gesture",
                                "tick": state.tick,
                                "op": "drag-drop",
                                "disposition": "refused",
                                "outcome": "drag-drop-refused",
                                "kind": "outside-work-area",
                                "window": current.token,
                                "producer": producer,
                            }),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(state, me, &fulls_owned, areas);
                        continue;
                    }
                    None
                }
                DropDestination::Cross { output, workspace } => Some((output, workspace, x, y)),
                DropDestination::Refused(kind) => {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "drag-drop",
                            "disposition": "refused",
                            "outcome": "drag-drop-refused",
                            "kind": kind,
                            "window": current.token,
                            "producer": producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    continue;
                }
            }
        } else {
            None
        };
        // Cross-output finishes snapshot their fences now: the bridged path
        // stashes the snapshot with the pending slot, the inline path below
        // re-reads live against it immediately. Unreadable topology refuses
        // with snap-back before any Engine call or queued slot.
        let cross_fence = match &cross_dest {
            Some((output, workspace, _, _)) => {
                let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
                let retained_full = retained
                    .iter()
                    .find(|r| r.key == member_key)
                    .map(|r| r.fullscreen)
                    .unwrap_or(false);
                let probe = CrossDragSettle {
                    mover_token: current.token.clone(),
                    member_key: member_key.clone(),
                    source_output: loc.output.clone(),
                    source_workspace: loc.workspace.clone(),
                    dest_output: output.clone(),
                    dest_workspace: workspace.clone(),
                    x: 0,
                    y: 0,
                    drop_prior: None,
                    producer: producer.to_owned(),
                    // Fence probe only: no Engine event is built from it.
                    start_rect: None,
                };
                match snapshot_cross_drag(
                    state,
                    areas,
                    &probe,
                    live_max,
                    live_full_opt.unwrap_or(retained_full),
                ) {
                    Some(fence) => Some(fence),
                    None => {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "gesture",
                                "tick": state.tick,
                                "op": "drag-drop",
                                "disposition": "refused",
                                "outcome": "drag-drop-refused",
                                "kind": "unknown-output",
                                "window": current.token,
                                "producer": producer,
                            }),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(state, me, &fulls_owned, areas);
                        continue;
                    }
                }
            }
            None => None,
        };
        // Bridged finish: hide (the safety epoch bars any stale-show replay)
        // and stash the drop as a bounded pending owner operation serviced
        // from render acks at loop level. No blocking wait: the owner stays
        // responsive to stop and new events, and the drop runs only after
        // the proven `Hidden` acknowledgement, re-entered with fresh
        // observations and gates before every focus/Engine/foreign write
        // below. The same-thread path keeps its proven-inline ordering.
        if state.render_bridge.is_some() {
            let pending_dest = match (&cross_dest, &cross_fence) {
                (Some((output, workspace, _, _)), Some(fence)) => Some(PendingCrossDest {
                    output: output.clone(),
                    workspace: workspace.clone(),
                    fence: fence.clone(),
                }),
                _ => None,
            };
            let occupied = queue_pending_drop(
                state,
                hwnd,
                producer,
                intent,
                &current.token,
                start_key,
                start_tag,
                member_key,
                &loc.output,
                &loc.workspace,
                pending_dest,
                state.gesture_before.get(hwnd).copied(),
            );
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            state.gesture_end_cursor.remove(hwnd);
            state.windrag_start_cursor.remove(hwnd);
            state.gesture_before.remove(hwnd);
            if occupied {
                // A previous finish is still pending (overlapping END races):
                // refuse the newcomer narrowly with no drop and no
                // reconcile; the pending operation owns the settle.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": match &intent {
                            GestureIntent::MoveDrop { .. } => "drag-drop",
                            GestureIntent::PointerResize { .. } => "pointer-resize",
                        },
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-busy",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
            }
            continue;
        }
        // Inline cross-output settle (same-thread path, no render bridge):
        // fresh suspend/elevated gates, immediate fence re-read, proven
        // inline hide, then the shared Engine route. The pre-drop windrag
        // focus below stays same-output only: cross drops follow-focus on
        // the destination after the transfer instead of foregrounding the
        // source first.
        if let (Some((dest_output, dest_workspace, x, y)), Some(fence)) =
            (cross_dest.as_ref(), cross_fence.as_ref())
        {
            if state.suspended
                || suspend_read(state, me, fulls).veto.block
                || foreground_elevated(me)
            {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": if state.suspended {
                            "suspended"
                        } else {
                            "elevated-foreground"
                        },
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            let membership_ok = state
                .workspaces
                .member_loc(&member_key)
                .is_some_and(|fresh| {
                    fresh.output == loc.output && fresh.workspace == loc.workspace
                });
            let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
            let retained_full = retained
                .iter()
                .find(|r| r.key == member_key)
                .map(|r| r.fullscreen)
                .unwrap_or(false);
            let live = drag_drop_live(
                state,
                areas,
                fence,
                &member_key,
                membership_ok,
                live_max,
                live_full_opt.unwrap_or(retained_full),
            );
            if let Some(outcome) = crate::workspace_owner::drag_drop_fences_hold(fence, &live) {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": outcome,
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            // Proven inline hide before any Engine or project mutation: the
            // drop never executes over a possibly visible preview.
            if !hide_preview_finish(state, "finish") {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-unacked",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                state.preview_bound.remove(hwnd);
                state.preview_dead.remove(hwnd);
                state.gesture_preview_start.remove(hwnd);
                state.gesture_start_cursor.remove(hwnd);
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            let drop_prior = state
                .preview_bound
                .get(hwnd)
                .and_then(|bound| bound.hover_prior.clone());
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            state.tick += 1;
            let tick = state.tick;
            let correlation = state.correlation();
            let settle = CrossDragSettle {
                mover_token: current.token.clone(),
                member_key: member_key.clone(),
                source_output: loc.output.clone(),
                source_workspace: loc.workspace.clone(),
                dest_output: dest_output.clone(),
                dest_workspace: dest_workspace.clone(),
                x: *x,
                y: *y,
                drop_prior,
                producer: producer.to_owned(),
                start_rect: state.gesture_before.get(hwnd).copied(),
            };
            settle_cross_drag_drop(
                state,
                me,
                fulls,
                areas,
                &observed,
                &retained,
                &settle,
                fence,
                tick,
                &correlation,
                correlation.as_str(),
            );
            continue;
        }
        // Project-driven Win+Left focus binding (KDE mover parity): the
        // gesture owns the mover even when native focus lags (unfocused
        // Win+Left starts without focusing). The Engine binds logical focus
        // to the mover itself, but the native foreground needs the existing
        // safe focus authority (E8-prime plus attach plus one setter with an
        // exact readback; never the underlay `focused_window` value). Every
        // gate above (START identity/lifetime fence, member match,
        // sticky/float refusal, domain routing, same-output fence) passed
        // before this effect. A failed actuation refuses with snap-back and
        // no plan instead of dropping under a diverged foreground.
        if producer == "windrag" {
            let actuation = actuate_focus(state, me, fulls, &observed, &retained, &current.token);
            if actuation.outcome != "focus-ok" {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "refused",
                        "outcome": "gesture-refused-focus",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "windrag-focus",
                    "tick": state.tick,
                    "phase": "drop",
                    "outcome": "focus-ok",
                    "window": current.token,
                    "producer": producer,
                }),
            );
        }
        state.tick += 1;
        let correlation = state.correlation();
        // Incomplete snapshot defers with retained Engine state.
        let mut hint_cx = HintCx::new();
        let Some(rows) = assemble_domain_rows(
            state,
            &loc.output,
            &loc.workspace,
            &observed,
            &retained,
            "gesture",
            correlation.as_str(),
            &mut hint_cx,
        ) else {
            continue;
        };
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let mover = WindowId(current.token.clone());
        // Live gap edits adopt here so the drop convergence below sees
        // matching gaps instead of refusing the gesture.
        {
            let revision = revision_for(state, &loc.output, &loc.workspace);
            let outer_gap = state.outer_gap;
            adopt_gaps_for_route(
                state,
                &domain,
                &domain_key,
                outer_gap,
                &windows,
                Some(&mover),
                revision,
                fp,
                &correlation,
            );
        }
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &loc.output, &loc.workspace),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            Some(&mover),
        );
        // Single-domain observations run the local retained
        // propose/commit path; Core owns gesture semantics.
        // The preview track carried the exact sticky group-edge hover prior
        // across samples through the same resolver; the final drop forwards
        // that exact value (or `None` when no preview resolved, e.g. an
        // unmoved-then-dropped pointer with no samples). Preview/drop
        // agreement is by construction: same observation assembly, same
        // resolver, same carried prior.
        let drop_prior = state
            .preview_bound
            .get(hwnd)
            .and_then(|bound| bound.hover_prior.clone());
        // Closed bounded trace descriptor (`none` or `group-edge:Side`):
        // proves preview-to-drop carry equality without raw NodeIds.
        let drop_prior_desc = preview_prior_desc(&drop_prior);
        // Reply rendering must never outlive the gesture: the preview is
        // proven hidden before the Engine drop runs, so no late sample can
        // paint over the settled layout. With a render bridge this hides
        // with a verified `Hidden` acknowledgement the owner waits for
        // (bounded, outside callbacks) before the Engine drop below. An
        // unacknowledged hide refuses the drop narrowly (tentative stage 2
        // semantics: `drag-drop-refused` with kind `preview-finish-unacked`,
        // converging through one bounded reconcile like any other refusal):
        // the drop never executes over a possibly visible preview, and the
        // bumped safety epoch still bars any stale-show replay.
        if !hide_preview_finish(state, "finish") {
            let op = match &intent {
                GestureIntent::MoveDrop { .. } => "drag-drop",
                GestureIntent::PointerResize { .. } => "pointer-resize",
            };
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": "preview-finish-unacked",
                    "window": current.token,
                    "producer": producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        state.preview_bound.remove(hwnd);
        state.preview_dead.remove(hwnd);
        state.gesture_preview_start.remove(hwnd);
        state.gesture_start_cursor.remove(hwnd);
        let reply = match intent {
            GestureIntent::MoveDrop { x, y } => {
                let mut event = event;
                event.command = CoreCommand::DragDrop {
                    window: current.token.clone(),
                    x,
                    y,
                    hover_prior: drop_prior,
                    source: None,
                };
                handle_engine_event(state, &mut event)
            }
            GestureIntent::PointerResize {
                direction,
                boundary,
                direction2,
                boundary2,
            } => {
                let mut event = event;
                event.command = CoreCommand::PointerResize {
                    window: current.token.clone(),
                    direction: direction.to_owned(),
                    boundary,
                    direction2: direction2.map(str::to_owned),
                    boundary2,
                };
                handle_engine_event(state, &mut event)
            }
        };
        let op = match intent {
            GestureIntent::MoveDrop { .. } => "drag-drop",
            GestureIntent::PointerResize { .. } => "pointer-resize",
        };
        match &reply {
            CoreReply::Tiled(_) | CoreReply::Projection(_) | CoreReply::Resize(_) => {
                let tick = state.tick;
                let fulls_owned = fulls.to_vec();
                let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
                apply_geometry(
                    state,
                    ApplyInput {
                        me,
                        fulls: &fulls_owned,
                        reply: &reply,
                        observed: &observed,
                        op,
                        tick,
                        correlation: correlation.as_str(),
                        skipped: Vec::new(),
                        writable: &writable,
                        output_token: state.workspaces.output_token(&loc.output),
                        workspace_token: state
                            .workspaces
                            .workspace_token(&loc.output, &loc.workspace),
                        revision: revision_for(state, &loc.output, &loc.workspace),
                    },
                );
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "applied",
                        "outcome": if op == "drag-drop" { "drag-drop-applied" } else { "pointer-resize-applied" },
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
            }
            _ => {
                // Refusal converges through one bounded reconcile (snap-back
                // to the retained allocation), matching the KDE
                // restore-marker outcome with no ledger. Self, centre-stack,
                // and unsupported targets land here with no plan and no
                // commit; the closed Engine kind rides along for attribution.
                let kind = match &reply {
                    CoreReply::Rejected { kind, .. } => *kind,
                    CoreReply::Diverged(reason) => reason.as_str(),
                    CoreReply::SnapshotInvalid { detail, .. } => *detail,
                    _ => "unhandled-variant",
                };
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": kind,
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
        }
    }
    state.gesture_before.clear();
    state.esc_latched.clear();
    state.gesture_esc_seq.clear();
    state.gesture_end_seq.clear();
    state.gesture_end_cursor.clear();
    // A pending bridged finish owns its START snapshots until the loop-level
    // service consumes them: retain exactly that HWND's entries here.
    let pending_hwnd = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.hwnd);
    match pending_hwnd {
        Some(hwnd) => {
            state.gesture_start_key.retain(|key, _| *key == hwnd);
            state.gesture_start_tag.retain(|key, _| *key == hwnd);
        }
        None => {
            state.gesture_start_key.clear();
            state.gesture_start_tag.clear();
        }
    }
    state.gesture_producer.clear();
    state.windrag_start_cursor.clear();
    state.windrag_bound.clear();
    state.preview_bound.clear();
    state.preview_dead.clear();
    state.gesture_preview_start.clear();
    state.gesture_start_cursor.clear();
    // No gesture remains open: no preview rectangle survives the settle.
    hide_preview(state, "finish");
}

/// Native foreground veto read: one fresh pass over the live foreground with
/// exact desktop identity, fresh visibility, caption, DWM frame, and DWM
/// cloak facts fed into the portable [`crate::workspace_owner::classify_foreground`] policy.
/// No titles, paths, PIDs, or content leave this function; the caller logs
/// only the returned reason plus booleans. Pure reads, never IO.
struct ForegroundRead {
    veto: crate::workspace_owner::ForegroundVeto,
    desktop: bool,
    visible: bool,
    captioned: bool,
    dwm_readable: bool,
    cloaked: bool,
}

fn foreground_read(fulls: &[Rect]) -> ForegroundRead {
    use crate::workspace_owner::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
    let settled = |veto: crate::workspace_owner::ForegroundVeto,
                   desktop: bool,
                   visible: bool,
                   captioned: bool,
                   dwm_readable: bool,
                   cloaked: bool| {
        ForegroundRead {
            veto,
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        }
    };
    let allow = |reason: ForegroundVetoReason,
                 desktop: bool,
                 visible: bool,
                 captioned: bool,
                 dwm_readable: bool,
                 cloaked: bool| {
        settled(
            crate::workspace_owner::ForegroundVeto {
                block: false,
                reason,
            },
            desktop,
            visible,
            captioned,
            dwm_readable,
            cloaked,
        )
    };
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        // No foreground window: nothing covering.
        return allow(
            ForegroundVetoReason::None,
            false,
            false,
            false,
            false,
            false,
        );
    }
    if unsafe { IsWindow(foreground) } == 0 {
        // `IsWindowVisible` returns false for invalid handles, so validity is
        // checked first and fails closed.
        return settled(
            crate::workspace_owner::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    // Exact known desktop shell handles are the desktop, not a fullscreen
    // application. Narrow API identity only: no class-name, exe, tool/popup,
    // or unmanaged exceptions.
    let shell = unsafe { GetShellWindow() };
    let desktop_handle = unsafe { GetDesktopWindow() };
    let is_desktop = (!shell.is_null() && shell == foreground)
        || (!desktop_handle.is_null() && desktop_handle == foreground);
    // Fresh unambiguous visibility read on the valid handle: invisible
    // windows cannot be covering fullscreen.
    let visible = unsafe { IsWindowVisible(foreground) } != 0;
    let style = unsafe {
        SetLastError(0);
        GetWindowLongW(foreground, GWL_STYLE)
    } as u32;
    let style_err = unsafe { GetLastError() };
    let style_ok = style != 0 || style_err == 0;
    let captioned = style & WS_CAPTION != 0;
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    let dwm_ok = unsafe {
        DwmGetWindowAttribute(
            foreground,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0;
    // Fresh cloak fact on the same valid handle: a cloaked foreground is
    // invisible to the compositor and never a covering fullscreen, even
    // with a monitor-spanning frame. Query failure fails closed (unreadable)
    // like the frame read, matching the admission and hold gates.
    let mut cloak_val: i32 = 0;
    let cloak_ok = unsafe {
        DwmGetWindowAttribute(
            foreground,
            DWMWA_CLOAKED,
            (&mut cloak_val as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } == 0;
    if unsafe { IsWindow(foreground) } == 0 {
        return settled(
            crate::workspace_owner::ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Invalid,
            },
            false,
            false,
            false,
            false,
            false,
        );
    }
    let readable = dwm_ok && style_ok;
    let covers_monitor = readable
        && rect_from_win(visible_raw)
            .is_some_and(|visible| is_borderless_fullscreen(true, visible, fulls));
    let veto = classify_foreground(ForegroundFacts {
        valid: true,
        is_desktop,
        visible,
        captioned,
        dwm_readable: readable,
        cloak_readable: cloak_ok,
        cloaked: cloak_val != 0,
        covers_monitor,
    });
    settled(
        veto,
        is_desktop,
        visible,
        captioned,
        readable,
        cloak_val != 0,
    )
}

/// Managed-aware suspend read: the raw foreground veto, except a real
/// fullscreen foreground verified as a managed member or a born-held
/// first-seen fullscreen rides a retained overlay with no geometry writes
/// and never suspends the workspace (`ManagedOverlay`, like a managed
/// maximize which keeps its caption and never vetoes). Unverified fullscreen
/// still suspends, and unreadable/invalid foregrounds always stay blocked:
/// a non-covering or unreadable window is never labeled a managed overlay.
/// The managed branch reuses the single `holdable_key` verification
/// (identity, session, medium integrity, safety gates, scope, hosted child,
/// proof ownership) plus membership and the live lifetime tag, so no gate
/// is duplicated. Born tracking honors the first-seen gate: a managed
/// overlay transition (slotted member or lifetime-known non-fullscreen)
/// never re-enters the hold.
fn suspend_read(state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect]) -> ForegroundRead {
    #[cfg(test)]
    if let Some(blocked) = INJECTED_SUSPEND.with(|slot| slot.get()) {
        use crate::workspace_owner::ForegroundVetoReason;
        let _ = (state, me, fulls);
        if blocked {
            return ForegroundRead {
                veto: crate::workspace_owner::ForegroundVeto {
                    block: true,
                    reason: ForegroundVetoReason::Fullscreen,
                },
                desktop: false,
                visible: true,
                captioned: false,
                dwm_readable: true,
                cloaked: false,
            };
        }
        return ForegroundRead {
            veto: crate::workspace_owner::ForegroundVeto {
                block: false,
                reason: ForegroundVetoReason::None,
            },
            desktop: false,
            visible: false,
            captioned: false,
            dwm_readable: false,
            cloaked: false,
        };
    }
    use crate::workspace_owner::ForegroundVetoReason;
    let raw = foreground_read(fulls);
    if !raw.veto.block {
        return raw;
    }
    if raw.veto.reason != ForegroundVetoReason::Fullscreen {
        return raw;
    }
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 {
        return raw;
    }
    let managed = || ForegroundRead {
        veto: crate::workspace_owner::ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::ManagedOverlay,
        },
        desktop: raw.desktop,
        visible: raw.visible,
        captioned: raw.captioned,
        dwm_readable: raw.dwm_readable,
        cloaked: raw.cloaked,
    };
    let Some(held_key) = holdable_key(state, me, foreground) else {
        return raw;
    };
    let tag_ok = {
        let live_tag = crate::product_hide::sys::read_member_tag(foreground);
        state.member_tags.get(&held_key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        })
    };
    if state.workspaces.member_loc(&held_key).is_some()
        && state.member_tokens.contains_key(&held_key)
        && tag_ok
    {
        return managed();
    }
    let known_slot = state
        .member_tokens
        .get(&held_key)
        .is_some_and(|token| state.member_rects.contains_key(token));
    if !should_hold_born_fullscreen(
        true,
        known_slot,
        state.seen_nonfullscreen.contains(&held_key),
    ) {
        return raw;
    }
    managed()
}

/// Owned inputs for one `run_tile_loop` invocation. Bundled so the loop
/// entry keeps a narrow signature as keyboard policy joins the run.
/// `mouse_snap` arms session-only `SPI_SETWINARRANGING FALSE` prevention for
/// product runs; proof runs always pass false (no settings changes).
struct TileRun {
    seconds: Option<u64>,
    trace: bool,
    allowlist: Option<Vec<AllowEntry>>,
    proof: bool,
    /// `shortcut-proof` only: proof geometry gate stays on, but the owner
    /// installs the hook with test-only marker acceptance and drives the same
    /// keyboard dispatcher plus session-only mouse routines. Product `tile`
    /// and `tile-proof` always pass false.
    shortcut_proof: bool,
    /// `workspace-proof` only: like `shortcut-proof` plus the workspace
    /// dispatcher (select/send/follow with hide/reveal) for exactly the
    /// frozen allowlist. Never enabled on the product path.
    workspace_proof: bool,
    raw_argv: Vec<String>,
    keyboard: KeyboardConfig,
    mouse_snap: bool,
    /// First-run note for the `tile-start` log (`None` on proof paths, which
    /// never prompt): the chosen preset plus whether it persisted.
    first_run: Option<String>,
    /// Normal owner only (`None` on proof paths): the parsed options plus the
    /// pre-lease settings state, resolved into the effective options under
    /// the owner lease before any effect or hook (so a refused second owner
    /// never prompts and a mid-prompt file race never overwrites).
    first_run_pending: Option<FirstRunPending>,
    /// Live inner/outer gaps for this run (settings base, CLI overrides).
    inner_gap: i32,
    outer_gap: i32,
    /// Settings directory for live polling (`None` disables polling: proof
    /// owners never load the user's store).
    settings_dir: Option<std::path::PathBuf>,
    /// Initialized last-good live settings (`None` disables polling).
    settings_live: Option<LiveSettings>,
    /// Explicit startup CLI switches, authoritative for the whole run (empty
    /// on proof paths, which never poll the file).
    cli_overrides: crate::tiling::CliOverrides,
    /// Explicit normal-mode scope filter (empty means no filter). Proof runs
    /// always pass empty (frozen allowlist is the gate there).
    scope: Vec<String>,
    /// Explicit host-to-child scope pairs (empty means no child constraint).
    /// Proof runs always pass empty.
    scope_hosts: Vec<ScopeHostChild>,
    border: ActiveBorderOptions,
    underlay: GroupUnderlayOptions,
}

/// Apply validated live settings through the owner's pump (normal owner
/// only). Returns true when owner state changed, so the caller wakes the pump
/// and gap adoption reconciles promptly. Gaps flip immediately and adopt
/// through the retained update-gaps path in `reconcile_tick` (plain reconcile
/// would refuse the drift); border/underlay assign directly for the next
/// overlay refresh; keyboard plus rebound/suppression chords route through the
/// hook's `apply_live_config` (in-flight holds keep their down-time verdict
/// and mask, the stale queue drains, nothing dispatches under the new
/// config); mouse prevention toggles through the existing conditional
/// restore (off) and fresh-preimage disable (on) routines, flag-only while
/// suspended so the resume transition drives the effect.
///
/// Status semantics are honest by construction: `saved:{revision}` means the
/// file revision is assigned to owner state (per-lane evidence rides the
/// `settings-applied` lanes plus the `settings-keyboard` row); the mouse
/// effect rides the existing suspend/resume outcome logs (`mouse-pending`
/// while suspended defers it to resume); gap adoption rides the Engine
/// update-gaps trace. Invalid files keep last-good with a `degraded:{reason}`
/// status and are never rewritten here.
///
/// Explicit startup CLI switches stay authoritative: the file base applies
/// underneath [`crate::tiling::CliOverrides`], so an unrelated file edit
/// never drops a CLI lane.
#[allow(clippy::too_many_arguments)]
fn apply_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
    settings: crate::settings::Settings,
    mtime: Option<std::time::SystemTime>,
) -> bool {
    let log_path = state.log_path.clone();
    let mut base = crate::settings::tile_options_from_settings(&settings);
    crate::tiling::apply_cli_overrides(&mut base, &state.cli_overrides);
    let mut lanes: Vec<&'static str> = Vec::new();
    let mut changed = false;
    if base.inner_gap != state.inner_gap || base.outer_gap != state.outer_gap {
        state.inner_gap = base.inner_gap;
        state.outer_gap = base.outer_gap;
        lanes.push("gaps");
        changed = true;
    }
    // Live default edits adopt for future workspaces only: existing session
    // overrides keep their state, exactly like startup seeding.
    if state
        .workspaces
        .set_default_tiled(settings.core.workspace.default_tiled)
    {
        lanes.push("workspace-default");
        changed = true;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-default",
                "default_tiled": settings.core.workspace.default_tiled,
                "outcome": "adopted",
                "scope": "future-workspaces",
            }),
        );
    }
    // Item 12 stage B: a saved scope-mode change never adopts live. The
    // pending choice stays in the saved document (assigned to `live.settings`
    // below) and takes effect only on the next owner restart; the drift lane
    // is informational and never wakes the pump.
    if let Some((lane, event)) =
        crate::workspace_scope_owner::pending_scope_lane(&state.scope_owner, &settings)
    {
        log_json_at(&log_path, event);
        lanes.push(lane);
    }
    if base.border != state.border {
        state.border = base.border;
        lanes.push("border");
        changed = true;
    }
    if base.underlay != state.underlay {
        state.underlay = base.underlay;
        lanes.push("underlay");
        changed = true;
    }
    let keyboard = KeyboardConfig {
        takeover: !base.no_keyboard_snap_takeover,
        allow_win_l: base.allow_win_l,
    };
    let remap = crate::settings::build_remap(&settings);
    let disabled = crate::settings::build_disabled(&settings);
    if keyboard != state.keyboard || remap != state.last_remap || disabled != state.last_disabled {
        state.keyboard = keyboard;
        state.last_remap = remap.clone();
        state.last_disabled = disabled.clone();
        if state.input_bridge.is_some() {
            // Product path: the policy rides the next versioned snapshot
            // (generations assigned at offer time) and the pump applies it in
            // FIFO order; the true stale-drained count arrives with the
            // `settings-keyboard-ack` event. No direct hook call: input TLS
            // lives on the pump thread. Dispatch of superseded batches is
            // gated owner-side by generation until the ack lands.
            lanes.push("keyboard");
            changed = true;
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-keyboard",
                    "takeover": keyboard.takeover,
                    "allow_win_l": keyboard.allow_win_l,
                    "remap": remap.len(),
                    "disabled": disabled.len(),
                    "drained": 0,
                    "ack_pending": true,
                }),
            );
        } else {
            let drained = crate::snapkey::sys::apply_live_config(keyboard, &remap, &disabled);
            lanes.push("keyboard");
            changed = true;
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "settings-keyboard",
                    "takeover": keyboard.takeover,
                    "allow_win_l": keyboard.allow_win_l,
                    "remap": remap.len(),
                    "disabled": disabled.len(),
                    "drained": drained,
                }),
            );
        }
    }
    let want_snap = !base.no_mouse_snap_prevention;
    if want_snap != *snap_want {
        // Mirror the suspend/resume fencing: while suspended the ledger
        // already holds the restored state, so only the flag flips and the
        // resume transition drives the effect.
        if want_snap {
            if !state.suspended {
                snap_resume(dir, me, store);
            }
        } else if !state.suspended {
            snap_suspend(dir, me, store);
        }
        *snap_want = want_snap;
        lanes.push(if state.suspended {
            "mouse-pending"
        } else {
            "mouse"
        });
        changed = true;
    }
    let revision = settings.revision;
    // R-MOV-03: the same-axis mode adopts here with no lane and no rebuild:
    // assigning `live.settings` below is enough, since `keyboard_tick` reads
    // the mode per move. A change to only this field returns false, so no
    // tree rebuild or resync follows; the next move simply uses the new mode.
    // R-MAX-06 rides the same assignment: `admit_slotless_maximized` binds
    // the born-maximized policy per fresh lifetime at admission, so a
    // setting-only change adopts for subsequent first admissions with no
    // rebuild, and already-bound lifetimes plus previously admitted clients
    // keep their slots and topology.
    // R-SPC-04 D1/D2: the fixed-size predicate adopts here for subsequent
    // admissions only. The Engine propagates the value to every retained
    // session without touching topology, revision, or existing
    // automatic/override marks (admission-only classification); the D4/D5
    // admission gates read it per fresh lifetime through the same
    // assignment. A change to only this field returns false, so no tree
    // rebuild or resync follows.
    let predicate = settings.core.fixed_size_predicate_mode();
    if state.engine.fixed_size_predicate() != predicate {
        state.engine.set_fixed_size_predicate(predicate);
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-predicate",
                "predicate": predicate.as_wire_str(),
                "outcome": "adopted",
                "scope": "subsequent-admissions",
            }),
        );
    }
    if let Some(live) = state.settings_live.as_mut() {
        let saved_scope = crate::workspace_scope_owner::saved_mode(&settings);
        let status = state.scope_owner.status_for(revision, saved_scope);
        live.settings = settings;
        live.mtime = mtime;
        live.status = status;
    }
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "settings-applied",
            "revision": revision,
            "lanes": lanes,
        }),
    );
    changed
}

/// Poll the settings file once per pump (normal owner only; proof owners
/// never poll). Changed files validate then apply (returns true when owner
/// state changed); invalid files keep the last-good values live with a
/// degraded status and are never rewritten here (no blind reset). Degraded
/// transitions log once per status.
fn poll_live_settings(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: &mut bool,
) -> bool {
    let Some(settings_dir) = state.settings_dir.clone() else {
        return false;
    };
    let Some(last) = state.settings_live.clone() else {
        return false;
    };
    match crate::settings::poll_for_change(&settings_dir, &last) {
        crate::settings::PollOutcome::Unchanged => false,
        crate::settings::PollOutcome::Changed(settings) => {
            let mtime = std::fs::metadata(settings_dir.join(crate::settings::SETTINGS_FILE_NAME))
                .and_then(|meta| meta.modified())
                .ok();
            apply_live_settings(state, dir, me, store, snap_want, settings, mtime)
        }
        crate::settings::PollOutcome::InvalidKept(reason) => {
            let degraded = format!("degraded:{reason}");
            let changed = state
                .settings_live
                .as_ref()
                .is_some_and(|live| live.status != degraded);
            if changed {
                if let Some(live) = state.settings_live.as_mut() {
                    live.status = degraded.clone();
                }
                log_json_at(
                    &state.log_path.clone(),
                    serde_json::json!({
                        "event": "settings-degraded",
                        "status": degraded,
                    }),
                );
            }
            false
        }
    }
}

fn run_tile_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    run: TileRun,
) -> Result<()> {
    let TileRun {
        seconds,
        trace,
        allowlist,
        proof,
        shortcut_proof,
        workspace_proof,
        raw_argv,
        mut keyboard,
        mut mouse_snap,
        mut first_run,
        first_run_pending,
        mut inner_gap,
        mut outer_gap,
        mut settings_dir,
        mut settings_live,
        cli_overrides,
        mut scope,
        mut scope_hosts,
        mut border,
        mut underlay,
    } = run;
    // Prevention needs an active loop: proof never arms it, and the guarded
    // setup already captured the preimage plus the initial effect. The live
    // settings poll may flip this per pump (normal owner only).
    let mut snap_want = mouse_snap && (!proof || shortcut_proof || workspace_proof);
    ensure_pm_v2()?;
    let owner = OwnerId::parse(OWNER_ID).expect("static owner token is valid");
    let generation =
        GenerationId::parse(&me.process_creation).ok_or_else(|| err("error: bad generation"))?;
    let log_path = crate::lifecycle::sys::log_path_for(dir, &me.process_creation);
    let audit_path: Option<std::path::PathBuf> = if proof {
        Some(dir.join(format!("proof-audit-{}.jsonl", me.process_creation)))
    } else {
        None
    };
    // First-run preset under the owner lease (normal owner only): the lease
    // refusal already stopped a second owner before any UI, and the resolved
    // choice reapplies every explicit CLI lane before effects or hooks.
    // Proof paths carry no pending state and keep deterministic config.
    if !proof && let Some(pending) = first_run_pending {
        let resolved = resolve_first_run(dir, me, pending, &cli_overrides, &log_path)?;
        keyboard = KeyboardConfig {
            takeover: !resolved.options.no_keyboard_snap_takeover,
            allow_win_l: resolved.options.allow_win_l,
        };
        mouse_snap = !resolved.options.no_mouse_snap_prevention;
        snap_want = mouse_snap;
        first_run = resolved.first_run;
        inner_gap = resolved.options.inner_gap;
        outer_gap = resolved.options.outer_gap;
        settings_dir = resolved.settings_dir;
        settings_live = resolved.live;
        scope = resolved.options.scope_exes.clone();
        scope_hosts = resolved.options.scope_hosts.clone();
        border = resolved.options.border;
        underlay = resolved.options.underlay;
    }
    // Proof audit starts before any geometry: raw received argv (elements
    // stay separate strings so spaces survive without quoting), parsed
    // seconds/trace/mode, and allowlist digest/count. The frozen identity
    // record follows verification below. Raw argv carries flags and paths
    // only (no titles or window content) and never enters production logs.
    if let Some(audit) = audit_path.as_ref() {
        if audit.exists() {
            return Err(err("refuse: audit preexists, will not overwrite"));
        }
        let digest = allowlist
            .as_ref()
            .map(|entries| allowlist_digest(entries))
            .unwrap_or_default();
        let count = allowlist.as_ref().map(|e| e.len()).unwrap_or(0);
        let line = serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": if workspace_proof {
                "workspace-proof"
            } else if shortcut_proof {
                "shortcut-proof"
            } else {
                "proof"
            },
            "allowlist_digest": digest,
            "allowlist_count": count,
        });
        log_json_at(audit, line);
    }
    // Proof gate before first geometry: every frozen entry must verify as an
    // owned helper (sibling exe/class/tag plus full identity). Invisible
    // passive members verify here and stay frozen until `show`.
    if proof {
        let entries = allowlist
            .as_ref()
            .ok_or_else(|| err("refuse: allowlist missing"))?;
        if entries.is_empty() {
            return Err(err("refuse: empty allowlist"));
        }
        for (index, entry) in entries.iter().enumerate() {
            verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
                err(format!(
                    "refuse: proof allowlist non-owned index={index} {reason}"
                ))
            })?;
        }
        if let Some(audit) = audit_path.as_ref() {
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
                audit,
                serde_json::json!({"event": "proof-frozen", "windows": frozen}),
            );
        }
    }
    let last_remap = settings_live
        .as_ref()
        .map(|live| crate::settings::build_remap(&live.settings))
        .unwrap_or_default();
    let last_disabled = settings_live
        .as_ref()
        .map(|live| crate::settings::build_disabled(&live.settings))
        .unwrap_or_default();
    let mut state = TileLoop {
        engine: Engine::new(),
        owner: owner.clone(),
        generation: generation.clone(),
        tokens: TokenMap::default(),
        refused: RefusedTracker::default(),
        stable: HashMap::new(),
        managed: HashSet::new(),
        active: HashSet::new(),
        gesture_before: HashMap::new(),
        gesture_end_cursor: HashMap::new(),
        gesture_start_key: HashMap::new(),
        gesture_start_tag: HashMap::new(),
        tick: 0,
        allowlist,
        scope,
        scope_hosts,
        trace,
        suspended: false,
        last_summary: None,
        log_path: log_path.clone(),
        audit_path: audit_path.clone(),
        keyboard,
        inner_gap,
        outer_gap,
        settings_live,
        settings_dir,
        last_remap,
        last_disabled,
        cli_overrides,
        snap_dropped: 0,
        cb_diag_dropped: 0,
        cb_diag_filtered: 0,
        cb_outbox_dropped: 0,
        mask_sends: 0,
        mask_send_max_us: 0,
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        gesture_producer: HashMap::new(),
        windrag_start_cursor: HashMap::new(),
        windrag_bound: HashMap::new(),
        windrag_dropped: 0,
        windrag_origin_logged: None,
        windrag_stats_logged: (0, 0, 0, 0),
        snap_advance: None,
        resize_repeat: None,
        last_enumerated: 0,
        workspaces: ManagedWorkspaces::new(),
        scope_owner: crate::workspace_scope_owner::WorkspaceScopeOwner::new(),
        member_tokens: std::collections::BTreeMap::new(),
        member_rects: HashMap::new(),
        hidden_claims: std::collections::BTreeMap::new(),
        member_identity: std::collections::BTreeMap::new(),
        member_tags: std::collections::BTreeMap::new(),
        active_output: String::new(),
        workspace_proof: false,
        maximize_admission_attempted: HashSet::new(),
        born_keep_overlay: BTreeSet::new(),
        retile_keep_overlay: BTreeSet::new(),
        born_fullscreen: BTreeSet::new(),
        seen_nonfullscreen: BTreeSet::new(),
        last_foreground: 0,
        known_outputs: Vec::new(),
        last_hwnds: HashSet::new(),
        minimized_omitted: HashSet::new(),
        pending_closed: std::collections::BTreeMap::new(),
        last_areas: Vec::new(),
        hint_logged: HashMap::new(),
        restore_wake: None,
        border,
        border_overlay: BorderOverlay::default(),
        border_last: None,
        underlay,
        underlay_overlay: UnderlayOverlay::default(),
        underlay_last: None,
        move_kind: HashMap::new(),
        gesture_start_cursor: HashMap::new(),
        preview_overlay: PreviewOverlay::default(),
        preview_last: None,
        preview_bound: HashMap::new(),
        gesture_preview_start: HashMap::new(),
        preview_dead: HashSet::new(),
        esc_latched: HashSet::new(),
        gesture_esc_seq: HashMap::new(),
        gesture_end_seq: HashMap::new(),
        underlay_chord_last: false,
        float_topmost_prev: std::collections::BTreeMap::new(),
        float_rects: HashMap::new(),
        floated: BTreeSet::new(),
        sticky: std::collections::BTreeMap::new(),
        fixed_tile_overrides: BTreeSet::new(),
        fixed_origin_overrides: BTreeSet::new(),
        fixed_auto_seen: BTreeSet::new(),
        born_first_exit: BTreeSet::new(),
        overlay_now: HashMap::new(),
        input_bridge: None,
        render_bridge: None,
        render_frames: RenderFrameHealth::default(),
        pending_releases: crate::workspace::PendingReleases::new(),
        pending_retry_fp: 0,
    };
    state.engine.sync_binding(&owner, &generation);
    state.workspace_proof = workspace_proof;
    // R-SPC-04 D1-D8 + G-05: enable the shared fixed-size admission opt-in
    // now that the adapter wires complete observation (bounded min/max
    // tracks with seeded-untouched stripping), lifetime-bound origin and
    // override stamps, and no-touch admission lanes. `Engine::new` defaults
    // OFF to preserve exact prior behavior; the predicate starts from the
    // last-good startup settings (both-axes default, proof owners included)
    // and later adopts live edits for subsequent admissions only.
    state.engine.set_fixed_size_admission(true);
    state
        .engine
        .set_fixed_size_predicate(fixed_predicate_for(state.settings_live.as_ref()));
    // Existing workspaces take the saved default at owner startup (initially
    // tiled); live default edits later affect only newly created workspaces.
    // Overrides reset on owner restart: nothing persists per workspace.
    let startup_default = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.workspace.default_tiled)
        .unwrap_or(true);
    state.workspaces.set_default_tiled(startup_default);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "workspace-default",
            "default_tiled": startup_default,
            "outcome": "adopted",
            "scope": "startup",
        }),
    );
    // Item 12 stage B: the saved scope mode adopts only on owner restart.
    // The adopted mode stays in force for the whole run; later live edits
    // report drift without adopting. Startup priming follows the initial
    // monitor sync; later hotplug retains its current behavior.
    let startup_scope = crate::workspace_scope_owner::startup_mode(state.settings_live.as_ref());
    state.scope_owner =
        crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(startup_scope);
    log_json_at(&log_path, state.scope_owner.startup_event());
    let mut areas = all_monitors()?;
    let (mut monitor, mut monitor_count) = (areas[0].clone(), areas.len());
    let mode = if workspace_proof {
        "workspace-proof"
    } else if shortcut_proof {
        "shortcut-proof"
    } else if state.allowlist.is_some() {
        "proof"
    } else {
        "normal"
    };
    // Visible takeover state: default on, explicit off, proof never hooks
    // except shortcut-proof/workspace-proof with test-only marker acceptance.
    let takeover = (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
    // Per-output-local workspaces start from live monitors: one domain set
    // per device key, retained Engine sessions per (output, workspace).
    sync_monitor_outputs(&mut state, &areas);
    // Item 12 stage B: scoped startup priming on the fixed topology (mode
    // order/history baselines plus shared current convergence and the
    // trailing floor). Runs once here; later hotplug syncs mode-aware
    // through `scope_hotplug`.
    scope_trailing::prime_after_sync(&mut state);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "tile-start",
            "mode": mode,
            "trace": trace,
            "monitors": monitor_count,
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "inner": state.inner_gap,
            "outer": state.outer_gap,
            "keyboard": {"takeover": takeover, "allow_win_l": state.keyboard.allow_win_l},
            "mouse_snap_prevention": snap_want,
            "settings": state.settings_live.as_ref().map(|live| live.status.clone()),
            "first_run": first_run,
            "scope_count": state.scope.len(),
            "active_border": {
                "enabled": state.border.enabled,
                "width": state.border.style.width,
                "gap": state.border.style.gap,
                "radius": state.border.style.radius,
                "color": crate::active_border::render_color(state.border.style.color),
                "use_theme": state.border.style.use_theme,
            },
        }),
    );
    // Hook on this thread; this thread pumps messages, so callbacks run here.
    // Four narrow hooks keep the callback cheap: foreground wakes, move/size
    // transitions queue losslessly, object create/destroy/show/hide wakes,
    // and explicit location-change wakes (a range up to LOCATIONCHANGE would
    // also pull unrelated state-change noise, so it stays separate).
    let hooks = unsafe {
        [
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_SYSTEM_MOVESIZESTART,
                EVENT_SYSTEM_MOVESIZEEND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW
                    .max(EVENT_OBJECT_DESTROY)
                    .max(EVENT_OBJECT_HIDE),
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
        ]
    };
    if hooks.iter().any(|hook| hook.is_null()) {
        for hook in hooks {
            if !hook.is_null() {
                unsafe {
                    UnhookWinEvent(hook);
                }
            }
        }
        return Err(err("error: SetWinEventHook failed"));
    }
    // Owner-owned notification tray (normal owner only; proof owners stay
    // deterministic with no icon). Created after the hooks so no hook-failure
    // early return can leak it, and RAII `Drop` covers every later path.
    // Creation failure degrades to tray-less tiling, never a refused run.
    // The hidden owner window rides the loop-thread pump below, so its
    // TaskbarCreated and tray callbacks dispatch on this same thread. The
    // conflict projection uses the effective runtime keyboard policy
    // (takeover off passes everything through, so nothing warns; the Win+L
    // opt-in follows the runtime allow_win_l) over the CLI-overridden
    // settings base.
    let mut tray: Option<crate::tray_sys::TrayOwner> = if proof {
        None
    } else {
        let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
            Some(live) => (
                crate::tray::unresolved_conflicts(
                    &live.settings,
                    state.keyboard.takeover,
                    state.keyboard.allow_win_l,
                )
                .is_empty(),
                live.status.clone(),
            ),
            None => (true, "saved:unknown".to_owned()),
        };
        // Session-only workspace toggle: the menu carries its rendered
        // opaque scope, the owner loop drains it once on its pump via
        // `take_toggle_request` and verifies it below. Default picks persist
        // through the settings file (normal owner).
        let toggle = Arc::new(Mutex::new(None::<crate::tray::ToggleScope>));
        // Stop-aware menu tracking: the modal track blocks this loop's pump,
        // so the menu thread observes the exact-owner stop file itself on a
        // 50 ms window timer and cancels via `EndMenu`. Same file plus match
        // predicate the loop polls below, so no ownership change. Read errors
        // poll false (never cancel on I/O noise); the loop's own check still
        // reports them.
        let menu_stop_dir = dir.to_path_buf();
        let menu_stop_creation = me.process_creation.clone();
        let menu_stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>> =
            Some(Arc::new(move || {
                match std::fs::read_to_string(
                    menu_stop_dir.join(crate::lifecycle::STOP_REQUEST_FILE),
                ) {
                    Ok(text) => crate::lifecycle::stop_request_matches(&text, &menu_stop_creation),
                    Err(_) => false,
                }
            }));
        match crate::tray_sys::TrayOwner::create(
            conflict_empty,
            &settings_status,
            Some(log_path.clone()),
            &toggle,
            state.settings_dir.clone(),
            menu_stop_poll,
        ) {
            Ok(owner) => Some(owner),
            Err(error) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"tray-unavailable","cause": error.to_string()}),
                );
                None
            }
        }
    };
    // Product keyboard takeover on the loop thread: the callback runs during
    // pump_wait on this same thread, so install/uninstall bracket the loop
    // with no join. Plain proof mode never installs a hook; shortcut-proof
    // installs with test-only marker acceptance. Install is best-effort with
    // bounded backoff retries below: failure degrades to keyboard-unavailable
    // (nothing consumes) while tiling continues, never a refused run.
    //
    // Product `tile` runs both hooks on one dedicated input pump instead
    // (stage 1): the callbacks never run on this owner thread, so owner
    // observation, Engine, actuation and I/O cannot delay delivery. Proof
    // commands keep the legacy same-thread hooks below untouched.
    let use_input_thread = !proof && !shortcut_proof && !workspace_proof;
    let mut snap_hook = None;
    let mut snap_failures: u32 = 0;
    let mut snap_retry_at: Option<Instant> = None;
    // Project Win+Left hook on the loop thread, same pump as the keyboard
    // hook. Installed once when takeover holds; normal runs also install it
    // while takeover is off so a live settings re-enable takes effect (the
    // gate below keeps everything passing through until then). Install
    // failure degrades to title-bar-only movement (Win+Left passes through
    // natively) while tiling continues, never a refused run.
    //
    // Product path: owned by the input pump instead (same install-while-off
    // contract, same pass-through gate via snapshots).
    let windrag_hook = if !use_input_thread && (takeover || !proof) {
        match crate::win_mouse::sys::install() {
            Ok(hook) => {
                log_json_at(&log_path, serde_json::json!({"event":"windrag-available"}));
                Some(hook)
            }
            Err(message) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"windrag-unavailable","cause": message}),
                );
                None
            }
        }
    } else {
        None
    };
    // Dedicated input pump for the product path: both LL hooks plus their
    // synchronous classifiers on one thread. Readiness uses a bounded wait;
    // install failures retry on the pump with the same 5s-doubling/60s-cap
    // vocabulary and the loop keeps tiling degraded meanwhile. A spawn
    // failure degrades the same way (no hooks, tiling continues).
    if use_input_thread {
        match InputHandle::spawn(state.keyboard) {
            Ok(mut handle) => {
                let ready = handle.wait_ready(READY_WAIT, &mut |event: &FailEvent| {
                    log_json_at(
                        &log_path,
                        if event.recovered {
                            serde_json::json!({
                                "event": if event.hook == "keyboard" { "snap-available" } else { "windrag-available" },
                                "producer": "input-pump",
                            })
                        } else {
                            serde_json::json!({
                                "event": if event.hook == "keyboard" { "snap-unavailable" } else { "windrag-unavailable" },
                                "attempt": event.attempt,
                                "retry_secs": event.retry_secs,
                                "producer": "input-pump",
                            })
                        },
                    );
                });
                if ready {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"input-ready","bounded_wait":"met"}),
                    );
                } else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"input-pending","note":"hooks retry on the input pump; tiling continues degraded"}),
                    );
                }
                let channels = handle.channels().clone();
                // Esc clocks and edges are process-global singletons sampled
                // directly by callbacks and WinEvent START/END; no per-handle
                // registration can mismatch them.
                // Fence generations start at the process-global maximum so a
                // second same-process owner run re-arms the callback fence
                // above every earlier allocation instead of idling below a
                // pinned `fetch_max` ceiling (a reset would race an active
                // pump; a leftover pending fence stays fail-closed).
                state.input_bridge = Some(InputBridge {
                    channels: Arc::clone(&channels),
                    sender: InboxSender::new(Arc::clone(&channels.inbox)),
                    handle,
                    last_snapshot: None,
                    retry_gen: None,
                    force_snapshot: false,
                    last_clear_seen: 0,
                    published_gen: fence_base_generation(),
                    applied_gen: 0,
                    last_faults: 0,
                    last_deduped: 0,
                    pending_ack: None,
                    last_ack_logged: 0,
                });
            }
            Err(cause) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"input-spawn-failed","cause": cause}),
                );
            }
        }
    }
    // Dedicated renderer for the product path: all three overlay carriers
    // move to one renderer thread (stage 2, option B). Readiness uses a
    // bounded wait; a spawn failure degrades to same-thread overlays while
    // tiling continues, never a refused run. Proof paths keep `None` above
    // and stay same-thread.
    if use_input_thread {
        match RenderHandle::spawn() {
            Ok(mut handle) => {
                let ready = handle.wait_ready(RENDER_READY_WAIT, &mut |event: &RenderFailEvent| {
                    log_json_at(
                        &log_path,
                        if event.recovered {
                            serde_json::json!({
                                "event": "render-available",
                                "op": event.op,
                                "producer": "render-thread",
                            })
                        } else {
                            let mut line = serde_json::json!({
                                "event": "render-unavailable",
                                "op": event.op,
                                "attempt": event.attempt,
                                "retry_secs": event.retry_secs,
                                "producer": "render-thread",
                            });
                            if let Some(detail) = event.detail.as_ref() {
                                line["cause"] = serde_json::Value::from(detail.clone());
                            }
                            line
                        },
                    );
                });
                if ready {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"render-ready","bounded_wait":"met"}),
                    );
                } else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"render-pending","note":"renderer retries creation on its pump; tiling continues with pending presents"}),
                    );
                }
                let channels = handle.channels().clone();
                state.render_bridge = Some(RenderBridge {
                    channels,
                    handle: Some(handle),
                    gens: [0, 0, 0],
                    hide_posted: [false, false, false],
                    hide_reason: [None, None, None],
                    hidden_ack: [None, None, None],
                    posted: std::array::from_fn(|_| VecDeque::new()),
                    applied_visible: [false, false, false],
                    applied_note: [None, None, None],
                    applied_ack: [None, None, None],
                    applied_known: [false, false, false],
                    admission_open: true,
                    pending: None,
                });
            }
            Err(cause) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"render-spawn-failed","cause": cause}),
                );
            }
        }
    }
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    let mut slow_last = Instant::now();
    // Trace-only health window (input ages, owner work, windrag transport):
    // plain local, never shared across threads. Each loop iteration lends
    // it to its `OwnerWorkGuard` for the turn; the pending end-of-run
    // summary flushes after the loop below. Untouched (and unborrowed)
    // when tracing is off, so default runs pay nothing here.
    let mut health = Health::new();
    let result = (|| -> Result<()> {
        let fulls = monitor_fulls(&areas);
        // Mouse prevention is deferred until an actual active tick or resume:
        // an initial fullscreen must not disable only to immediately restore.
        // The first active tick, resume, or work-area change drives the
        // effect; teardown and crash restore keep the lease-held guarantee.
        let mut snap_primed = false;
        // Fullscreen guard applies before the initial tick as well.
        // Verified managed overlays bypass like a managed maximize.
        if suspend_read(&mut state, me, &fulls).veto.block {
            state.suspended = true;
            state.snap_advance = None;
            log_json_at(
                &log_path,
                serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
            );
            if snap_want {
                snap_suspend(dir, me, store);
            }
        } else {
            if !areas
                .iter()
                .any(|a| tiling_domain_bounds_with(a.work, state.outer_gap).is_some())
            {
                return Err(err("error: work area cannot carry outer gap"));
            }
            if snap_want {
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            reconcile_tick(&mut state, me, &fulls, &areas);
            refresh_active_border(&mut state, me, &fulls);
            refresh_group_underlay(&mut state, me, &fulls, &areas);
        }
        // Trace-only owner-work sequence: counted only when tracing so
        // default runs pay no timer/counter work.
        let mut owner_seq: u64 = 0;
        // Last observed windrag offer count for change-only transport
        // observations (replaces the per-offer `windrag-transport` line).
        let mut trace_windrag_offers: u64 = 0;
        // Last observed pump turn/offer counts for change-only input
        // observations: a fresh turn (or batch/drop) samples the slot
        // once; an unchanged diag is never resampled. No per-turn file
        // writes: observations aggregate, only throttled outliers and
        // window summaries log.
        let mut trace_last_turns: u64 = 0;
        let mut trace_last_offers: u64 = 0;
        // Trace-only outer-wait return counts (outer product wait only;
        // focus/proof `pump_wait` calls never touch these). `waits` counts
        // outer returns, `ack_only` the ack-only subset that returns early
        // before slow maintenance, and the reason hits count `WaitWake` flags
        // per return (combinations possible: ack plus messages is common, so
        // reason hits can exceed `waits`). Native wait returns only, never
        // thread context switches. Counted only when tracing so default runs
        // pay no counter work.
        let mut ow_counts = OwnerWaitCounts::default();
        loop {
            if stop_requested(dir, me)? || tray.as_ref().is_some_and(|t| t.stop_requested()) {
                return Ok(());
            }
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Ok(());
            }
            let wait = pump_wait_with_intent(
                TICK_POLL_MS,
                state
                    .input_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.channels.intent_wake()),
                state
                    .render_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.handle.as_ref())
                    .map(|handle| handle.ack_wake()),
            );
            // Trace-only owner-work clock starts here: `busy_us` excludes the
            // wait above plus callbacks delivered inside it. Scoped to this
            // iteration so every `continue`/error path below still samples.
            // Outer-wait return accounting lives here too (trace-only): one
            // `ow_waits` per outer return plus one hit per set `WaitWake`
            // flag. The ack-only subset increments later at the early return,
            // once hook events are known.
            let mut owner_work = OwnerWorkGuard::begin(
                state.trace,
                &log_path,
                owner_seq,
                state.tick,
                if state.trace { Some(&mut health) } else { None },
            );
            if state.trace {
                owner_seq = owner_seq.saturating_add(1);
                ow_counts.note_wait(wait);
            }
            let events = hook_queue()
                .lock()
                .map(|mut queue| std::mem::take(&mut *queue))
                .unwrap_or_default();
            let mut woke = false;
            // Trace-only batch breakdown for the owner-work sample; default
            // runs skip the counting.
            let ow_batch_total: u64 = if state.trace { events.len() as u64 } else { 0 };
            let mut ow_wakes: u64 = 0;
            let mut ow_starts: u64 = 0;
            let mut ow_ends: u64 = 0;
            for event in events {
                match event {
                    HookEvent::Wake(raw) => {
                        // Own overlay events never wake the loop: no repeated
                        // full reconcile off our own move/paint/show.
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        if state.trace {
                            ow_wakes += 1;
                        }
                    }
                    HookEvent::MoveSizeStart(raw, start_seq) => {
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        if state.trace {
                            ow_starts += 1;
                        }
                        let hwnd = raw as u64;
                        // Shared START capture (Esc snapshot, PRE frame,
                        // identity fence); see `capture_gesture_start`. The
                        // single post-loop drain below latches edges for
                        // both held and just-ended gestures.
                        capture_gesture_start(&mut state, me, hwnd, start_seq);
                        state.active.insert(hwnd);
                        // Move-vs-resize classification, sampled once at START
                        // (the WinEvent itself does not distinguish). Hung or
                        // unreadable windows classify Unknown: never a trigger.
                        let kind = classify_move_size_start(hwnd);
                        state.move_kind.insert(hwnd, kind);
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "move-kind",
                                "tick": state.tick,
                                "kind": match kind {
                                    MoveSizeKind::Move => "move",
                                    MoveSizeKind::Resize => "resize",
                                    MoveSizeKind::Unknown => "unknown",
                                },
                            }),
                        );
                        // START-time pointer for the preview zero gate: a
                        // title click with no pointer movement never shows.
                        // A failed read fails closed to no preview (the
                        // refresh hides on `no-start`).
                        if let Some(point) = cursor_pos() {
                            state.gesture_start_cursor.insert(hwnd, point);
                        } else {
                            state.gesture_start_cursor.remove(&hwnd);
                        }
                    }
                    HookEvent::MoveSizeEnd(raw, cursor, end_seq) => {
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        if state.trace {
                            ow_ends += 1;
                        }
                        let hwnd = raw as u64;
                        // Release cursor carried from the WinEvent callback,
                        // never re-read later. A missing capture fails closed
                        // to no-change for moves at settle time, so drop any
                        // stale entry rather than carrying it forward.
                        match cursor {
                            Some(point) => {
                                state.gesture_end_cursor.insert(hwnd, point);
                            }
                            None => {
                                state.gesture_end_cursor.remove(&hwnd);
                            }
                        }
                        // END-time Esc sequence, first END stands: the drain
                        // latches ended gestures only against this snapshot,
                        // never against a later counter read.
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            state.gesture_end_seq.entry(hwnd)
                        {
                            e.insert(end_seq);
                        }
                        state.active.remove(&hwnd);
                        // The START-sampled move/size kind travels to settle:
                        // `gesture_tick` reads and clears it exactly once per
                        // END, so caption-started moves survive a system
                        // rescale (mixed-DPI WM_DPICHANGED) that rect
                        // comparison alone would misread as a user resize.
                        // Preview and underlay keep reading the held entry
                        // until settle runs later this same turn.
                    }
                }
            }
            // Ack accounting runs on every turn (cheap: three mutex takes
            // plus the bounded failure drain), then the pending finish is
            // serviced from the ack state with no blocking wait.
            drain_render_failures(&mut state);
            poll_render_acks(&mut state, Instant::now());
            service_pending_drop(&mut state, me, &fulls, &areas);
            // Ack-only turns (renderer ack with no input, message, timeout,
            // or hook-callback event) loop here without reposting frames,
            // reconciling, polling settings, or touching input: an ack alone
            // never triggers owner work, so the ack wake cannot feed back
            // into a post/ack/post loop. Hook events above force a full
            // turn, so no input starves; the timeout preserves the timed
            // follow/reassert cadence, and the pending finish above still
            // completes with no added latency.
            if !(wait.full_turn() || woke) {
                if state.trace {
                    ow_counts.note_ack_only();
                }
                continue;
            }
            let now = Instant::now();
            let slow = now.duration_since(slow_last).as_millis() >= u128::from(SLOW_POLL_MS);
            // Best-effort keyboard hook with bounded backoff retries (5s
            // doubling, 60s cap): failure logs one `snap-unavailable` per
            // attempt, never per-poll noise, and the loop keeps tiling with
            // nothing consuming. Resume and work-area changes re-arm an
            // immediate retry; there is no permanent disable. The gate
            // follows the live takeover setting so a settings re-enable
            // installs the hook, and a fresh install publishes the live
            // rebound table immediately.
            let takeover_live =
                (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
            if state.input_bridge.is_some() {
                // Product path: the pump owns install/retry for both hooks.
                // Drain failure events for the owner writer; late readiness
                // completes here if the bounded startup wait expired first.
                // Then retry pending inbox payloads and poll the apply ack.
                if let Some(bridge) = state.input_bridge.as_mut() {
                    bridge.handle.drain_failures(&mut |event: &FailEvent| {
                        log_json_at(
                            &log_path,
                            if event.recovered {
                                serde_json::json!({
                                    "event": if event.hook == "keyboard" { "snap-available" } else { "windrag-available" },
                                    "producer": "input-pump",
                                })
                            } else {
                                serde_json::json!({
                                    "event": if event.hook == "keyboard" { "snap-unavailable" } else { "windrag-unavailable" },
                                    "attempt": event.attempt,
                                    "retry_secs": event.retry_secs,
                                    "producer": "input-pump",
                                })
                            },
                        );
                    });
                    let had = bridge.sender.pending_len() + bridge.sender.safety_len();
                    bridge.sender.flush();
                    // Retry payload moved into the inbox: wake the pump so a
                    // deadline-idle wait applies it promptly. Quiet when
                    // nothing was outstanding (no per-tick spurious wake).
                    if had > 0 {
                        bridge.channels.notify_work();
                    }
                    log_input_faults(bridge, &log_path);
                }
                poll_input_ack(&mut state);
            }
            // Renderer acknowledgements and lifecycle were already drained
            // above (before the ack-only branch), so ack accounting never
            // stalls behind the input gate: a failed input spawn must not
            // stall it, and an ack-only turn must not wait for it.
            if !use_input_thread
                && takeover_live
                && snap_hook.is_none()
                && snap_retry_at.is_none_or(|at| now >= at)
            {
                let installed = if shortcut_proof || workspace_proof {
                    crate::snapkey::sys::install_proof(state.keyboard)
                } else {
                    crate::snapkey::sys::install(state.keyboard)
                };
                match installed {
                    Ok(hook) => {
                        snap_hook = Some(hook);
                        // Trace-only collection starts here: the hook records
                        // nothing until this setter runs, so default runs pay
                        // no timing/filter/push work in the callback.
                        crate::snapkey::sys::set_callback_diag_enabled(state.trace);
                        // Publish the live policy plus the rebound/suppression
                        // tables onto the fresh machine (a fresh install never
                        // carries held keys, so nothing drains here).
                        let remap = state.last_remap.clone();
                        let disabled = state.last_disabled.clone();
                        crate::snapkey::sys::apply_live_config(state.keyboard, &remap, &disabled);
                        snap_failures = 0;
                        snap_retry_at = None;
                        log_json_at(&log_path, serde_json::json!({"event":"snap-available"}));
                    }
                    Err(message) => {
                        snap_failures += 1;
                        let backoff = (5u64 << snap_failures.min(4)).min(60);
                        snap_retry_at = Some(now + Duration::from_secs(backoff));
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "snap-unavailable",
                                "attempt": snap_failures,
                                "retry_secs": backoff,
                                "cause": message,
                            }),
                        );
                    }
                }
            }
            // Live settings poll (normal owner only; proof owners never
            // poll): validated edits apply BEFORE the hook-queue drain, so
            // `apply_live_config` drains stale intents before this pump
            // pulls them and nothing dispatches under the old config.
            // Invalid files keep last-good. A changed apply wakes the pump
            // so gap adoption reconciles promptly.
            if poll_live_settings(&mut state, dir, me, store, &mut snap_want) {
                woke = true;
            }
            // Tray follows live settings on the same pump: badge and tooltip
            // track the validated effective bindings under the runtime
            // keyboard policy (takeover plus the runtime Win+L opt-in), and a
            // pending add retries through the state gate (no duplicate
            // icons). `sync` itself is change-gated.
            if let Some(tray) = tray.as_mut() {
                let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
                    Some(live) => (
                        crate::tray::unresolved_conflicts(
                            &live.settings,
                            state.keyboard.takeover,
                            state.keyboard.allow_win_l,
                        )
                        .is_empty(),
                        live.status.clone(),
                    ),
                    None => (true, "saved:unknown".to_owned()),
                };
                tray.sync(conflict_empty, &settings_status);
                // Truthful workspace section: the opaque live scope plus its
                // tiled state and the persisted default. The next menu renders
                // exactly this; a toggle pick drains once below and flips the
                // mode on the loop thread only when the live active pair still
                // matches the rendered scope (stale/mismatched refuses with no
                // guessed fallback).
                let scope = if state.active_output.is_empty() {
                    None
                } else {
                    state
                        .workspaces
                        .active_id(&state.active_output)
                        .map(|workspace| (state.active_output.clone(), workspace))
                };
                let current = scope.as_ref().and_then(|(output, workspace)| {
                    state
                        .workspaces
                        .workspace_ids(output)
                        .iter()
                        .any(|id| id == workspace)
                        .then(|| state.workspaces.is_tiled(output, workspace))
                });
                tray.sync_workspace(scope, current, state.workspaces.default_tiled());
                if let Some(request) = tray.take_toggle_request() {
                    let live = if state.active_output.is_empty() {
                        None
                    } else {
                        state
                            .workspaces
                            .active_id(&state.active_output)
                            .map(|workspace| (state.active_output.clone(), workspace))
                    };
                    let rendered = Some((request.output.as_str(), request.workspace.as_str()));
                    let live_ref = live
                        .as_ref()
                        .map(|(output, workspace)| (output.as_str(), workspace.as_str()));
                    if crate::tray::verify_toggle_scope(rendered, live_ref) {
                        let toggle_fulls = monitor_fulls(&areas);
                        toggle_workspace_tiling(
                            &mut state,
                            me,
                            &toggle_fulls,
                            &areas,
                            &request.output,
                        );
                        woke = true;
                    } else {
                        // Stale or mismatched menu scope (output switch or a
                        // pruned workspace between render and pick): refuse
                        // with opaque tokens only, never raw scope ids.
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "workspace-mode",
                                "op": "toggle",
                                "outcome": "refused-stale-scope",
                            }),
                        );
                    }
                }
            }
            // Shortcut-handling gate for the callback: fresh downs consume
            // iff takeover holds with this gate active (the eventual Xbox
            // pause will clear it; until then publication stays takeover
            // only). Fullscreen suspension, gesture, elevated foreground,
            // and unmanaged/unadmitted/shell foreground never enter this
            // gate, so inactive tiling never pauses interception: those
            // chords still swallow at the hook and the owner rechecks fresh
            // identity/scope/elevation/fullscreen/gesture per intent before
            // any action, never manipulating protected windows. Hook-side
            // work stays cheap reads, never expensive syscalls per key.
            let snap_gate_active = state.keyboard.takeover;
            let snap_events = if state.input_bridge.is_some() {
                // Product path: offer the whole ordered payload when content
                // changed (no redundant snapshots, no queue flushes) and take
                // the single-outstanding tagged batch. Batches tagged older
                // than the published policy are stale-dropped through the
                // config-change path (masks still resolve) instead of
                // dispatching under a replaced policy; held pairs ride
                // stored verdicts either way.
                let candidate = crate::input_handoff::InputSnapshot {
                    generation: 0,
                    keyboard: state.keyboard,
                    remap: state.last_remap.clone(),
                    disabled: state.last_disabled.clone(),
                    snap_origins: state.snap_origins.clone(),
                    windrag_origins: state.windrag_origins.clone(),
                    active: snap_gate_active,
                    trace: state.trace,
                };
                offer_input_snapshot(&mut state, candidate);
                let taken: Option<crate::input_handoff::TaggedBatch<QueuedSnapEvent>> =
                    state.input_bridge.as_ref().and_then(|bridge| {
                        let taken = bridge.channels.snap_outbox.take();
                        // Freed outbox space: wake the pump so parked
                        // pending batches and retained queue terminals retry
                        // promptly instead of stranding behind an idle wait.
                        if taken.is_some() {
                            bridge.channels.notify_work();
                        }
                        taken
                    });
                let mut batch = Vec::new();
                // Fresh take-time age for the health window (trace-only);
                // `None` unless a batch is taken below, so stale ages are
                // never recorded.
                let mut snap_age_us: Option<u64> = None;
                if let Some(tagged) = taken {
                    if tagged.is_empty() {
                        // Keep-alive empty: nothing to dispatch.
                    } else {
                        woke = true;
                        let published = state
                            .input_bridge
                            .as_ref()
                            .map(|bridge| bridge.published_gen)
                            .unwrap_or(0);
                        if tagged.is_stale(published) {
                            keyboard_tick(
                                &mut state,
                                me,
                                &fulls,
                                &areas,
                                tagged.items,
                                Some("config-change"),
                            );
                        } else {
                            if state.trace {
                                snap_age_us = Some(crate::input_pump::snap_batch_age_us(
                                    &tagged.items,
                                    Instant::now(),
                                ));
                            }
                            batch = tagged.items;
                        }
                    }
                }
                // Trace-only input health from the bounded diag slot: drop
                // transitions keep their explicit `snap-drop` line, while
                // turn samples aggregate into the window (a taken batch, a
                // fresh drop, or any fresh pump turn: ordinary pump turns
                // carry dispatch/gap/pending evidence even with no batch).
                // Snap age is `Some` only for a freshly taken batch;
                // pump age is `Some` only for a fresh offer (the slot
                // value persists from the last offer); heartbeat/pending
                // are last-observed values. The heartbeat proves liveness;
                // native timestamp vs delivery rides the summary-only
                // `delivery_gap_ms` aggregate, tick-count domain.
                if state.trace
                    && let Some(bridge) = state.input_bridge.as_mut()
                {
                    let diag = bridge.channels.diag.snapshot();
                    let drops_changed = diag.snap_dropped != state.snap_dropped;
                    if drops_changed {
                        let lost = diag.snap_dropped.saturating_sub(state.snap_dropped);
                        state.snap_dropped = diag.snap_dropped;
                        log_json_at(
                            &log_path,
                            serde_json::json!({"event":"snap-drop","lost": lost}),
                        );
                    }
                    let turns_changed = diag.turns != trace_last_turns;
                    let offers_changed = diag.offers != trace_last_offers;
                    if !batch.is_empty() || drops_changed || turns_changed {
                        trace_last_turns = diag.turns;
                        if offers_changed {
                            trace_last_offers = diag.offers;
                        }
                        let pump_age_us = if offers_changed {
                            Some(diag.queue_age_us)
                        } else {
                            None
                        };
                        owner_work.observe_input(&diag, snap_age_us, pump_age_us);
                    }
                }
                batch
            } else if snap_hook.is_some() {
                crate::snapkey::sys::publish_gate(&state.snap_origins, snap_gate_active);
                let batch = crate::snapkey::sys::drain_up_to(MAX_DISPATCH_PER_TICK);
                if !batch.is_empty() {
                    woke = true;
                }
                batch
            } else {
                Vec::new()
            };
            // Workspace digits, relative sends, explicit output sends and
            // history ride the same hook/queue/mask authority but dispatch
            // through the workspace owner, never the directional Engine
            // route. Partition here so each batch keeps its verdict
            // vocabulary; `shortcut-proof` keeps workspace hides disabled
            // inside `workspace_tick` via the proof gate.
            let mut workspace_events = Vec::new();
            let mut send_events = Vec::new();
            let mut output_events = Vec::new();
            let mut migrate_events: Vec<QueuedWorkspaceMigrateIntent> = Vec::new();
            let mut history_events = Vec::new();
            let mut directional_events = Vec::new();
            for event in snap_events {
                match event {
                    QueuedSnapEvent::Workspace(intent) => workspace_events.push(intent),
                    QueuedSnapEvent::WorkspaceSend(intent) => send_events.push(intent),
                    QueuedSnapEvent::WorkspaceOutput(intent) => output_events.push(intent),
                    QueuedSnapEvent::WorkspaceMigrate(intent) => migrate_events.push(intent),
                    QueuedSnapEvent::WorkspaceHistory(intent) => history_events.push(intent),
                    QueuedSnapEvent::Mask(_)
                    | QueuedSnapEvent::Intent(_)
                    | QueuedSnapEvent::Resize(_)
                    | QueuedSnapEvent::Maximize(_)
                    | QueuedSnapEvent::Orientation(_)
                    | QueuedSnapEvent::WorkspaceTiling(_)
                    | QueuedSnapEvent::Fullscreen(_)
                    | QueuedSnapEvent::Float(_)
                    | QueuedSnapEvent::Launch(_)
                    | QueuedSnapEvent::Sticky(_) => {
                        directional_events.push(event);
                    }
                }
            }
            if !workspace_events.is_empty()
                || !send_events.is_empty()
                || !output_events.is_empty()
                || !migrate_events.is_empty()
                || !history_events.is_empty()
            {
                woke = true;
            }
            let snap_events = directional_events;
            // Project Win+Left edges ride the same pump and gate: the hook
            // consumes downs/ups only while takeover holds, and every bound
            // candidate revalidates against the last complete observation in
            // `windrag_down` before anything arms. Downs pause tiling through
            // the shared `active` set (frames stay at source: no mid-gesture
            // writes); ups feed the shared `ended` settle below.
            //
            // Product path: edges arrive tagged from the input pump outbox.
            // Drop loss clears through ordered inbox invalidations so hook
            // disarm and owner state stay in FIFO order with snapshots.
            // Batches tagged under a superseded policy are stale: Downs are
            // refused without arming (a newer policy is live), while Ups and
            // Cancels still flow (owner-side closes are no-ops without open
            // gestures, preserving pair hygiene).
            let windrag_edges: Vec<WinDragEdge> = if state.input_bridge.is_some() {
                let taken: Option<crate::input_handoff::TaggedBatch<WinDragEdge>> =
                    state.input_bridge.as_ref().and_then(|bridge| {
                        let taken = bridge.channels.windrag_outbox.take();
                        // Freed outbox space: wake the pump so retained
                        // terminal Up/Cancel edges retry promptly instead of
                        // stranding behind an idle wait.
                        if taken.is_some() {
                            bridge.channels.notify_work();
                        }
                        taken
                    });
                let mut edges = Vec::new();
                if let Some(tagged) = taken
                    && !tagged.is_empty()
                {
                    woke = true;
                    let published = state
                        .input_bridge
                        .as_ref()
                        .map(|bridge| bridge.published_gen)
                        .unwrap_or(0);
                    if tagged.is_stale(published) {
                        let mut stale_downs = 0u32;
                        for edge in tagged.items {
                            match edge.kind {
                                WinDragKind::Down => stale_downs += 1,
                                WinDragKind::Up | WinDragKind::Cancel => {
                                    edges.push(edge);
                                }
                            }
                        }
                        if stale_downs > 0 {
                            log_json_at(
                                &log_path,
                                serde_json::json!({
                                    "event": "windrag-stale",
                                    "cause": "config-change",
                                    "dropped": stale_downs,
                                }),
                            );
                        }
                    } else {
                        edges = tagged.items;
                    }
                }
                // Trace-only windrag transport into the health window
                // (change-only offers; hook stats and drop lines below
                // stay explicit). The transport covers pump offer to
                // owner take only, never queue wait or native delivery.
                if state.trace {
                    let (offers, last_offer) = state
                        .input_bridge
                        .as_ref()
                        .map(|bridge| {
                            let diag = bridge.channels.diag.snapshot();
                            (diag.windrag_offers, diag.windrag_last_offer)
                        })
                        .unwrap_or((0, None));
                    if offers != trace_windrag_offers {
                        trace_windrag_offers = offers;
                        if let Some(offered) = last_offer {
                            owner_work.observe_windrag(offered);
                        }
                    }
                }
                if let Some(bridge) = state.input_bridge.as_ref() {
                    let diag = bridge.channels.diag.snapshot();
                    let stats = diag.hook_stats;
                    if stats != state.windrag_stats_logged {
                        state.windrag_stats_logged = stats;
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "windrag-hook-stats",
                                "tick": state.tick,
                                "downs_seen": stats.0,
                                "downs_consumed": stats.1,
                                "ups_seen": stats.2,
                                "ups_consumed": stats.3,
                            }),
                        );
                    }
                    if diag.windrag_dropped > state.windrag_dropped {
                        let lost = diag.windrag_dropped - state.windrag_dropped;
                        state.windrag_dropped = diag.windrag_dropped;
                        log_json_at(
                            &log_path,
                            serde_json::json!({"event":"windrag-drop","lost": lost}),
                        );
                        if !state.windrag_bound.is_empty() {
                            let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                            for hwnd in open {
                                clear_windrag_gesture(&mut state, hwnd);
                                if !offer_input_control(
                                    &mut state,
                                    InboxItem::InvalidateWindrag(hwnd),
                                ) {
                                    log_json_at(
                                        &log_path,
                                        serde_json::json!({
                                            "event": "windrag-drop-loss-unreported",
                                            "note": "invalidation held for retry; paired Up stays swallowed",
                                        }),
                                    );
                                }
                            }
                            log_json_at(
                                &log_path,
                                serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                            );
                            let fulls_owned = fulls.to_vec();
                            reconcile_tick(&mut state, me, &fulls_owned, &areas);
                        }
                    }
                }
                edges
            } else if windrag_hook.is_some() {
                crate::win_mouse::sys::publish(&state.windrag_origins, snap_gate_active);
                let stats = crate::win_mouse::sys::hook_stats();
                if stats != state.windrag_stats_logged {
                    state.windrag_stats_logged = stats;
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "windrag-hook-stats",
                            "tick": state.tick,
                            "downs_seen": stats.0,
                            "downs_consumed": stats.1,
                            "ups_seen": stats.2,
                            "ups_consumed": stats.3,
                        }),
                    );
                }
                let edges = crate::win_mouse::sys::drain_up_to(WINDRAG_MAX_DISPATCH_PER_TICK);
                if !edges.is_empty() {
                    woke = true;
                }
                edges
            } else {
                Vec::new()
            };
            // Legacy drop-loss path (same-thread hooks only): the product
            // pump reports loss above through the diag slot and ordered
            // inbox invalidations, so this block stays proof/legacy only.
            if !use_input_thread && windrag_hook.is_some() {
                let dropped = crate::win_mouse::sys::queue_dropped();
                if dropped > state.windrag_dropped {
                    let lost = dropped - state.windrag_dropped;
                    state.windrag_dropped = dropped;
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"windrag-drop","lost": lost}),
                    );
                    // A lost edge may have carried a Down whose gesture is
                    // open or an Up that would have closed one: discard every
                    // affected open project gesture rather than risk a
                    // stranded hold or a journey without its close. The hook
                    // is disarmed for them; their paired Ups stay swallowed.
                    if !state.windrag_bound.is_empty() {
                        let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                        for hwnd in open {
                            clear_windrag_gesture(&mut state, hwnd);
                            crate::win_mouse::sys::invalidate(hwnd);
                        }
                        log_json_at(
                            &log_path,
                            serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(&mut state, me, &fulls_owned, &areas);
                    }
                }
            }
            // Settle the drained edges (both paths): press-focus observation
            // plus the shared windrag_down/up/cancel vocabulary. Only runs
            // when edges wait; legacy and pump batches share this code.
            if !windrag_edges.is_empty() {
                // Press-focus observation for drained Downs: one fresh complete
                // enumeration per pump (only when a Down waits) feeds the
                // existing focus authority inside `windrag_down`. Token
                // minting is idempotent with the later phase observations;
                // an enumeration failure leaves empty slices so Downs refuse
                // closed with no arm and no writes.
                let mut press_observed: Vec<ObservedWindow> = Vec::new();
                let mut press_retained: Vec<RetainedRow> = Vec::new();
                if windrag_edges.iter().any(|e| e.kind == WinDragKind::Down) {
                    let mut skipped: Vec<(String, String)> = Vec::new();
                    if let Some(fresh) =
                        state.observe(me, &fulls, &mut skipped, &mut press_retained)
                    {
                        press_observed = fresh;
                    }
                }
                for edge in windrag_edges {
                    match edge.kind {
                        WinDragKind::Down => match windrag_down(
                            &mut state,
                            me,
                            &edge,
                            &fulls,
                            &press_observed,
                            &press_retained,
                        ) {
                            Ok(token) => {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-down",
                                        "tick": state.tick,
                                        "window": token,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                            Err(reason) => {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-down-refused",
                                        "tick": state.tick,
                                        "reason": reason,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                        },
                        WinDragKind::Up => {
                            windrag_up(&mut state, edge.hwnd, edge.x, edge.y, edge.esc_seq);
                        }
                        WinDragKind::Cancel => {
                            // Hook-side disarm after a consumed Down: drop
                            // the owner gesture with no plan and no geometry
                            // change. The paired Up stays swallowed
                            // hook-side; native gestures are untouched.
                            if let Some(token) = clear_windrag_gesture(&mut state, edge.hwnd) {
                                log_json_at(
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-cancel",
                                        "tick": state.tick,
                                        "outcome": "gesture-no-change",
                                        "window": token,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            // Proof-only callback diagnostics for shortcut-proof: accepted
            // marked events as the callback saw them, drained to the proof
            // audit (never production logs) so live runs can distinguish
            // actual modifier delivery from classifier state. Product `tile`
            // never records or writes these.
            if (shortcut_proof || workspace_proof) && snap_hook.is_some() {
                let diag = crate::snapkey::sys::drain_marked_diag();
                if !diag.is_empty()
                    && let Some(audit) = state.audit_path.as_ref()
                {
                    let entries: Vec<serde_json::Value> = diag
                        .iter()
                        .map(crate::snapkey::marked_diag_evidence)
                        .collect();
                    log_json_at(
                        audit,
                        serde_json::json!({
                            "event": "proof-keys",
                            "entries": entries,
                            "dropped": crate::snapkey::sys::marked_diag_dropped(),
                        }),
                    );
                }
                // Proof-only complement: non-marked modifier traffic
                // (injected-filtered plus physical, modifiers only) drained
                // to the proof audit under its own event, so the actual
                // callback modifier sequence is complete. Product `tile`
                // never records or writes these.
                let mods = crate::snapkey::sys::drain_mod_diag();
                if !mods.is_empty()
                    && let Some(audit) = state.audit_path.as_ref()
                {
                    let entries: Vec<serde_json::Value> =
                        mods.iter().map(crate::snapkey::mod_diag_evidence).collect();
                    log_json_at(
                        audit,
                        serde_json::json!({
                            "event": "proof-mods",
                            "entries": entries,
                            "dropped": crate::snapkey::sys::mod_diag_dropped(),
                        }),
                    );
                }
            }
            // Trace-only callback summary for product and proof paths: closed
            // vocabulary plus verdict/source/state/local-timing only (no
            // HWNDs, PIDs, tokens, titles). One bounded line per tick at
            // most; losses stay visible via dropped/filtered even when the
            // batch is empty. Mask `SendInput` latency rides the separate
            // `mask_sends`/`mask_send_max_us` aggregate (never inside
            // `duration_us`, never the downstream hook chain). Product entries
            // ride the pump `cb_outbox`; counts ride the diag slot. Neither
            // measures native delivery latency.
            let trace_has_pump = state.input_bridge.is_some();
            if state.trace && (snap_hook.is_some() || trace_has_pump) {
                let (diags, dropped, filtered, mask_sends, mask_send_max_us, cb_outbox_dropped) =
                    if state.input_bridge.is_some() {
                        match state.input_bridge.as_ref() {
                            Some(bridge) => {
                                let diag = bridge.channels.diag.snapshot();
                                let batch = bridge.channels.cb_outbox.take();
                                // Freed trace slot: wake the pump so a parked
                                // callback summary retries promptly.
                                if batch.is_some() {
                                    bridge.channels.notify_work();
                                }
                                let batch = batch.unwrap_or_default();
                                (
                                    batch,
                                    diag.cb_dropped,
                                    diag.cb_filtered,
                                    diag.mask_sends,
                                    diag.mask_send_max_us,
                                    diag.cb_outbox_dropped,
                                )
                            }
                            None => (Vec::new(), 0, 0, 0, 0, 0),
                        }
                    } else {
                        (
                            crate::snapkey::sys::drain_callback_diag(),
                            crate::snapkey::sys::callback_diag_dropped(),
                            crate::snapkey::sys::callback_diag_filtered(),
                            crate::snapkey::sys::mask_send_stats().0,
                            crate::snapkey::sys::mask_send_stats().1,
                            0,
                        )
                    };
                if !diags.is_empty()
                    || dropped != state.cb_diag_dropped
                    || filtered != state.cb_diag_filtered
                    || mask_sends != state.mask_sends
                    || mask_send_max_us != state.mask_send_max_us
                    || cb_outbox_dropped != state.cb_outbox_dropped
                {
                    state.cb_diag_dropped = dropped;
                    state.cb_diag_filtered = filtered;
                    state.mask_sends = mask_sends;
                    state.mask_send_max_us = mask_send_max_us;
                    state.cb_outbox_dropped = cb_outbox_dropped;
                    let max_duration_us = diags.iter().map(|d| d.duration_us).max().unwrap_or(0);
                    let entries: Vec<serde_json::Value> = diags
                        .iter()
                        .map(crate::snapkey::callback_diag_evidence)
                        .collect();
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "snap-callback",
                            "entries": entries,
                            "dropped": dropped,
                            "filtered": filtered,
                            "cb_outbox_dropped": cb_outbox_dropped,
                            "max_duration_us": max_duration_us,
                            "mask_sends": mask_sends,
                            "mask_send_max_us": mask_send_max_us,
                        }),
                    );
                }
            }
            if slow {
                slow_last = now;
                let fresh = all_monitors()?;
                let devices: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
                let fresh_devices: Vec<String> = fresh.iter().map(|a| a.device.clone()).collect();
                let geometry_changed = fresh.len() != monitor_count
                    || fresh_devices != devices
                    || fresh.iter().any(|a| {
                        areas
                            .iter()
                            .find(|b| b.device == a.device)
                            .is_none_or(|b| b.work != a.work || b.full != a.full)
                    });
                if geometry_changed {
                    areas = fresh;
                    monitor = areas[0].clone();
                    monitor_count = areas.len();
                    // Arrival/removal relocates whole-workspace Engine
                    // sessions; bounds-only changes reproject retained
                    // sessions without touching topology.
                    sync_monitor_outputs(&mut state, &areas);
                    for output in state.workspaces.output_keys() {
                        let Some(active) = state.workspaces.active_id(&output) else {
                            continue;
                        };
                        if let Some((_, key)) = workspace_domain_for(
                            &output,
                            &active,
                            &areas,
                            state.inner_gap,
                            state.outer_gap,
                        ) && let Some(area) = areas.iter().find(|a| a.device == output)
                            && let Some(bounds) =
                                tiling_domain_bounds_with(area.work, state.outer_gap)
                        {
                            state.engine.reproject_retained(&key, bounds);
                        }
                    }
                    log_json_at(&log_path, serde_json::json!({"event":"work-area-changed"}));
                    snap_retry_at = None;
                    // A changed work area is a meaningful retry point for a
                    // degraded Snap setup; quiet when the effect holds.
                    if snap_want && !state.suspended {
                        snap_resume(dir, me, store);
                        snap_primed = true;
                    }
                    woke = true;
                }
                // Trace-only pump-wait summary at the existing slow cadence:
                // one cumulative line per slow tick, no new timer or events.
                // Reads only (diag snapshot plus renderer atomics/ack seq);
                // never arms owner work and never signals a pump wake, so the
                // summary cannot induce a wake. Owner counts cover outer
                // product waits only (focus/proof nested waits excluded by
                // construction); ack-only turns counted in `owner_ack_only`
                // return before this block, so this line rides full turns.
                // Input `turns` counts completed iterations and equals
                // `input_waits`; stop before the wait increments neither;
                // `*_timeout` fields count genuine `WAIT_TIMEOUT` (258)
                // expiries only while `*_failed` counts `WAIT_FAILED` /
                // unrecognized codes separately (a failed wait returns
                // immediately; not proven delayed). Reason splits count native
                // wait returns only, never context switches; render acks ride
                // `render_acks` (`ack_seq`), never the wait counts. The input
                // struct arrives under one diag-slot lock (one coherent
                // snapshot); the renderer tuple is four independent atomic
                // loads, so a concurrent bump can skew the sum by one, and
                // cross-pump coherence is not claimed. Idle diffs take
                // start/end lines from one traced run; stop short-circuits
                // (owner return, input early return, renderer counted stop
                // wait) are the only lifecycle exclusions.
                if state.trace {
                    let (in_turns, in_waits, in_work, in_msg, in_timeout, in_failed) =
                        match state.input_bridge.as_ref() {
                            Some(bridge) => {
                                let diag = bridge.channels.diag.snapshot();
                                (
                                    diag.turns,
                                    diag.waits,
                                    diag.wait_work,
                                    diag.wait_msg,
                                    diag.wait_timeout,
                                    diag.wait_failed,
                                )
                            }
                            None => (0, 0, 0, 0, 0, 0),
                        };
                    let (re_waits, re_work, re_msg, re_timeout, re_failed, re_acks) =
                        match state.render_bridge.as_ref() {
                            Some(bridge) => {
                                let (waits, work, msg, timeout, failed) =
                                    bridge.channels.wait_counts();
                                (waits, work, msg, timeout, failed, bridge.channels.ack_seq())
                            }
                            None => (0, 0, 0, 0, 0, 0),
                        };
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "pump-waits",
                            "owner_waits": ow_counts.waits,
                            "owner_ack_only": ow_counts.ack_only,
                            "owner_input": ow_counts.input,
                            "owner_ack": ow_counts.ack,
                            "owner_msg": ow_counts.msg,
                            "owner_timeout": ow_counts.timeout,
                            "owner_failed": ow_counts.failed,
                            "input_turns": in_turns,
                            "input_waits": in_waits,
                            "input_work": in_work,
                            "input_msg": in_msg,
                            "input_timeout": in_timeout,
                            "input_failed": in_failed,
                            "render_waits": re_waits,
                            "render_work": re_work,
                            "render_msg": re_msg,
                            "render_timeout": re_timeout,
                            "render_failed": re_failed,
                            "render_acks": re_acks,
                            "scope": "outer-only-plus-native-waits",
                            "excludes": "focus-proof-nested-waits,stop-short-circuit",
                        }),
                    );
                }
            }
            // Exact-owner out-of-hook select wakes the loop even with no other
            // event; the poll itself consumes once and dispatches through the
            // existing resolver. Checked before the idle skip so a pending
            // request never waits for an unrelated wake.
            if dir.join(WORKSPACE_REQUEST_FILE).exists() || dir.join(RESIZE_REQUEST_FILE).exists() {
                woke = true;
            }
            // Bare-chord edge wake: GetAsyncKeyState levels are sampled here,
            // before the idle skip, so a Win+Shift press/release with no
            // WinEvent wakes the visual refresh on the 100ms pump cadence
            // instead of waiting for the 2s slow poll. Steady hold stays
            // quiet; the suspend/fullscreen path below still gates the
            // refresh, and tiling reconciliation keeps its normal pacing
            // (only transition ticks reconcile). No hook, no new framework.
            {
                let (win_held, shift_held) = chord_held_now();
                let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
                if state.underlay_chord_last != chord {
                    state.underlay_chord_last = chord;
                    woke = true;
                }
            }
            // Armed restore demands a reconcile once observed; the wake
            // survives pending dispatches and gesture/suspend pauses and is
            // consumed only by a gated reconcile below, on expiry, or on loss.
            if state.restore_wake.is_some() {
                let (expired, lost, done) =
                    restore_wake_probe(&state, state.restore_wake.as_ref().expect("armed"), now);
                let (keep, demand) =
                    crate::tiling::restore_wake_step(true, expired, lost, done, false);
                if demand {
                    woke = true;
                }
                if !keep {
                    state.restore_wake = None;
                }
            }
            // An open managed gesture wakes the 100ms pump for the
            // drop-preview track: pointer moves coalesce into one Engine
            // preview sample per tick on both the native frame-following
            // and the stationary Win+Left producers. The refresh itself
            // gates resize/unknown/invalid holds; the wake only needs START
            // capture.
            if !woke
                && state.active.iter().any(|hwnd| {
                    state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd)
                })
            {
                woke = true;
            }
            // Trace-only owner-work gate: quiet short-poll iterations drop the
            // disabled guard silently above via `continue`. Active wakes arm
            // the single sample; RAII then records suspend/held/error exits.
            // Due window summaries also flush on the idle path below (no new
            // wake: the check rides this existing turn, and quiet windows
            // with no activity emit nothing).
            if state.trace && (woke || slow) {
                let gesture_held = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
                owner_work.arm(ow_batch_total, ow_wakes, ow_starts, ow_ends, gesture_held);
            }
            // Per-turn maintenance-quiet flag for the trailing health
            // flush at the end of the turn (set on the quiet branch).
            let mut maintenance_quiet = false;
            if state.trace && !(woke || slow) {
                owner_work.finish_turn(false);
            }
            if !(woke || slow) {
                continue;
            }
            let fulls = monitor_fulls(&areas);
            if suspend_read(&mut state, me, &fulls).veto.block {
                if !state.suspended {
                    state.suspended = true;
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
                    );
                    if snap_want {
                        snap_suspend(dir, me, store);
                    }
                } else {
                    state.snap_advance = None;
                }
                if !snap_events.is_empty() {
                    keyboard_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        snap_events,
                        Some("suspended"),
                    );
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("suspended"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("suspended"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("suspended"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("suspended"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("suspended"),
                    );
                }
                // Out-of-hook select observes the same suspension: consumed
                // once with a suspended outcome, never applied while a
                // fullscreen foreground holds the session. The test-needed
                // resize poll carries its own suspend fence and refuses the
                // same way.
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                state.gesture_before.clear();
                state.active.clear();
                state.move_kind.clear();
                state.esc_latched.clear();
                state.gesture_esc_seq.clear();
                state.gesture_end_seq.clear();
                state.gesture_end_cursor.clear();
                state.gesture_start_key.clear();
                state.gesture_start_tag.clear();
                state.gesture_producer.clear();
                state.windrag_start_cursor.clear();
                state.windrag_bound.clear();
                state.preview_bound.clear();
                state.preview_dead.clear();
                state.gesture_preview_start.clear();
                state.gesture_start_cursor.clear();
                // A stale Esc edge must not leak into the next gesture.
                // Routes through the input bridge on the product path
                // (ordered FIFO with snapshots; held pairs keep their stored
                // verdicts until hook release) and clears the process-global
                // edge flags in pump order via the ClearEsc apply; direct
                // sys calls on proof paths. Atomic sequencing guarantees a
                // pre-gesture edge never cancels a later gesture.
                offer_input_control(&mut state, InboxItem::ClearEsc);
                // A suspended session disarms the project hold with no
                // replay; the consumed click already passed, so no half
                // gesture can strand later.
                offer_input_control(&mut state, InboxItem::ClearWindrag);
                hide_border(&mut state, "suspended");
                hide_underlay(&mut state, "suspended");
                hide_preview(&mut state, "suspended");
                // Suspended turns still commit their owner sample (the
                // guard would otherwise record it silently on drop with
                // no summary flush); then the disarmed guard drops.
                if state.trace {
                    owner_work.finish_turn(false);
                }
                continue;
            }
            if state.suspended {
                state.suspended = false;
                log_json_at(&log_path, serde_json::json!({"event":"resume"}));
                // A fresh foreground may accept the hook now: retry soon.
                snap_retry_at = None;
                // Fresh Snap preimage when the setting drifted during
                // suspension (or setup degraded earlier); quiet when the
                // effect is still ours. This is also the deferred initial
                // effect when the run started suspended.
                if snap_want {
                    snap_resume(dir, me, store);
                    snap_primed = true;
                }
            } else if snap_want && !snap_primed {
                // Deferred initial effect: the run started active but setup
                // no longer disables. Drive once on the first active tick.
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            // Border follows the live foreground on every active wake,
            // including managed gestures that pause tiling below: the refresh
            // reads one fresh frame and never waits for reconciliation.
            refresh_border_timed(&mut state, me, &fulls, &mut owner_work);
            // The group fill follows on the same wake: chord/move trigger over
            // the Engine-projected source union, same freshness contract.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
            // Only gestures on managed windows pause tiling; unrelated
            // windows never stall the loop.
            let ended: Vec<u64> = state
                .gesture_before
                .keys()
                .copied()
                .filter(|hwnd| state.managed.contains(hwnd) && !state.active.contains(hwnd))
                .collect();
            // Drop pre-gesture state for windows that left management.
            state
                .gesture_before
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_end_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_key
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_tag
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_esc_seq
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_end_seq
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_producer
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_start_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_bound
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .preview_bound
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .preview_dead
                .retain(|hwnd| state.managed.contains(hwnd));
            state
                .gesture_preview_start
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .gesture_start_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state.active.retain(|hwnd| state.managed.contains(hwnd));
            // Sequence-bound Esc latch, drained once per pump after event and
            // suspend handling. Still-held gestures compare the live counter
            // against START. Just-ended gestures compare the END-callback
            // snapshot against START instead: a physical Esc that lands after
            // the END but before this drain must never cancel the completed
            // drop, while a fast cancel tap arriving with the END batch still
            // latches. Only edges newer than the START snapshot latch, so
            // pre-gesture taps never cancel. Fast taps arrive via the prompt
            // callback, never via level polling. Injected Esc never sets the
            // native edge (product hook filters injected keys), so synthetic
            // title-bar proof paths keep their no-change restore without
            // claiming an explicit cancel; the project windrag edge below is
            // the one deliberate injected exception (the stationary hold has
            // no native modal loop to cancel it).
            {
                // Product path consumes the process-global edges published
                // directly in hook callbacks (no pump-forwarding delay can
                // hide an Esc landing just before a WinEvent END); sequences
                // come from the global atomic clocks sampled directly, never
                // a delayed batch. Cancellation still requires a newer
                // sequence than START, so stale flags never cancel.
                let (esc_hit, now_seq, windrag_hit, windrag_now) = if state.input_bridge.is_some() {
                    (
                        crate::input_handoff::global_esc_edge()
                            .swap(false, std::sync::atomic::Ordering::SeqCst),
                        crate::snapkey::sys::shared_esc_seq(),
                        crate::input_handoff::global_windrag_esc_edge()
                            .swap(false, std::sync::atomic::Ordering::SeqCst),
                        crate::snapkey::sys::shared_windrag_esc_seq(),
                    )
                } else {
                    (
                        crate::snapkey::sys::take_esc_edge(),
                        crate::snapkey::sys::esc_seq(),
                        crate::snapkey::sys::take_windrag_esc_edge(),
                        crate::snapkey::sys::windrag_esc_seq(),
                    )
                };
                if esc_hit {
                    for hwnd in state.active.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        let start_seq = state.gesture_esc_seq.get(hwnd).copied().unwrap_or(0);
                        if esc_edge_cancels(start_seq, now_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                    for hwnd in ended.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        // END-bound: absent snapshots never latch (fail
                        // closed to no-cancel; settle classifies from
                        // geometry, which a real cancel restored natively).
                        let (Some(start_seq), Some(end_seq)) = (
                            state.gesture_esc_seq.get(hwnd).copied(),
                            state.gesture_end_seq.get(hwnd).copied(),
                        ) else {
                            continue;
                        };
                        if esc_edge_cancels(start_seq, end_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                }
                // Project-gesture cancel edge (physical or injected Esc):
                // latches open windrag gestures only, with the same
                // START/END sequence ordering. Never admits a command;
                // settle reports the explicit `gesture-cancelled-esc`.
                if windrag_hit {
                    let now_seq = windrag_now;
                    for hwnd in state.active.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
                            continue;
                        }
                        let start_seq = state.gesture_esc_seq.get(hwnd).copied().unwrap_or(0);
                        if esc_edge_cancels(start_seq, now_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                    for hwnd in ended.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
                            continue;
                        }
                        let (Some(start_seq), Some(end_seq)) = (
                            state.gesture_esc_seq.get(hwnd).copied(),
                            state.gesture_end_seq.get(hwnd).copied(),
                        ) else {
                            continue;
                        };
                        if esc_edge_cancels(start_seq, end_seq) {
                            state.esc_latched.insert(*hwnd);
                        }
                    }
                }
            }
            // Logical cancel hides the preview IMMEDIATELY on latch, never
            // waiting for mouse-up (the stationary gesture has no native
            // modal loop to end it; the native loop ends on its own).
            for hwnd in state.esc_latched.iter().copied().collect::<Vec<_>>() {
                if state.preview_bound.remove(&hwnd).is_some()
                    || render_carrier_visible(&state, RenderCarrier::Preview)
                {
                    hide_preview(&mut state, "esc-cancelled");
                }
                state.preview_dead.remove(&hwnd);
                state.gesture_preview_start.remove(&hwnd);
                state.gesture_start_cursor.remove(&hwnd);
            }
            let paused = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
            if paused {
                // A managed gesture holds the loop and invalidates any
                // own-focus chain; keyboard intents of this batch are stale
                // and drop safely.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("gesture"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                // Drop-preview track on the 100ms pump while held: fresh
                // observation plus the shared Engine resolver per moved
                // pointer, on both producers. No focus, no geometry writes.
                refresh_drag_preview(&mut state, me, &fulls, &areas);
                // Due routine-frame summaries flush on this existing turn so
                // per-move preview aggregates never strand behind the hold.
                {
                    let now = Instant::now();
                    let lines = state.render_frames.take_due(now);
                    for line in lines {
                        log_json_at(&log_path, line);
                    }
                }
                continue;
            }
            if ended.is_empty() {
                if snap_events.is_empty()
                    && workspace_events.is_empty()
                    && send_events.is_empty()
                    && output_events.is_empty()
                    && migrate_events.is_empty()
                    && history_events.is_empty()
                {
                    reconcile_tick(&mut state, me, &fulls, &areas);
                    if state.restore_wake.is_some() {
                        let (expired, lost, done) = restore_wake_probe(
                            &state,
                            state.restore_wake.as_ref().expect("armed"),
                            Instant::now(),
                        );
                        let (keep, _) =
                            crate::tiling::restore_wake_step(true, expired, lost, done, true);
                        if !keep {
                            state.restore_wake = None;
                        }
                    }
                    workspace_maintenance(&mut state, me, store, dir, &fulls, &areas);
                    // Quiet-turn maintenance arms the trailing health
                    // flush at the end of the turn so pending activity
                    // never strands behind idle.
                    maintenance_quiet = true;
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                    poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                } else {
                    if !snap_events.is_empty() {
                        keyboard_tick(&mut state, me, &fulls, &areas, snap_events, None);
                    }
                    if !workspace_events.is_empty() {
                        workspace_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            workspace_events,
                            None,
                        );
                    }
                    if !send_events.is_empty() {
                        workspace_relative_send_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            send_events,
                            None,
                        );
                    }
                    if !output_events.is_empty() {
                        workspace_output_send_tick(
                            &mut state,
                            me,
                            &fulls,
                            &areas,
                            output_events,
                            None,
                        );
                    }
                    if !migrate_events.is_empty() {
                        workspace_migrate_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            migrate_events,
                            None,
                        );
                    }
                    if !history_events.is_empty() {
                        workspace_history_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            history_events,
                            None,
                        );
                    }
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                }
            } else {
                // A just-ended managed gesture settles before keyboard
                // continuation resumes.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("gesture"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                gesture_tick(&mut state, me, &fulls, &areas, &ended);
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
            }
            // Post-mutation border refresh: reconciliation may have moved the
            // target this tick, so re-read the fresh frame instead of leaving
            // the pre-reconcile geometry stale until the next wake. Own
            // overlay events are filtered above, so no feedback loop.
            refresh_border_timed(&mut state, me, &fulls, &mut owner_work);
            // Same for the group fill: a moved target may have joined another
            // group, so the projected union re-resolves instead of lingering.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
            // End-of-turn health commit: records the armed owner sample
            // (pre-log exclusive span, including this turn's refreshes)
            // and flushes due summaries, plus trailing activity on
            // maintenance-quiet turns. The guard then drops disarmed.
            if state.trace {
                owner_work.finish_turn(maintenance_quiet);
            }
            // Routine-frame summaries flush on this existing turn (no new
            // wake): due windows always, trailing activity on
            // maintenance-quiet turns. Runs in both trace and production
            // modes so aggregated geometry churn never strands.
            {
                let now = Instant::now();
                let lines = if maintenance_quiet {
                    state.render_frames.take_trailing(now)
                } else {
                    state.render_frames.take_due(now)
                };
                for line in lines {
                    log_json_at(&log_path, line);
                }
            }
        }
    })();
    // Pending end-of-run health summary (trace-only, at most one marked
    // line per stream): retained activity below the window age still
    // reports instead of dying with the loop.
    if state.trace {
        for line in health.take_end_of_run(Instant::now()) {
            log_json_at(&log_path, line);
        }
    }
    // Pending routine-frame activity flushes regardless of trace mode so
    // aggregated repaints never die with the loop.
    for line in state.render_frames.take_end_of_run(Instant::now()) {
        log_json_at(&log_path, line);
    }
    for hook in hooks {
        unsafe {
            UnhookWinEvent(hook);
        }
    }
    // Render admission closes first: invalidate every carrier epoch (a hide
    // on each carrier bumps the shared safety epoch and drops queued Shows)
    // and signal the renderer to stop, before the input stop wait. No stale
    // show can present during the potentially 5s input shutdown, and the
    // renderer observes stop promptly through its work wake.
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.admission_open = false;
        for carrier in RenderCarrier::all() {
            let index = carrier.index();
            if !bridge.hide_posted[index] {
                bridge.gens[index] += 1;
                let generation = bridge.gens[index];
                bridge.channels.hide(carrier, generation);
                bridge.hide_posted[index] = true;
            }
        }
        let _ = bridge.handle.as_mut().map(|handle| handle.request_stop());
    }
    // Product path graceful stop: close admission (owner stops pushing),
    // keep consumed pairs handled until the hook release, unhook on the
    // creator pump, and ack with a bounded wait. A missing ack returns an
    // unclean report without claiming clean and retains `Stopping` so no
    // overlapping worker can start; the existing recovery lifecycle is
    // untouched. Never waits indefinitely or holds owner locks while joining.
    // If this state is dropped after a timeout, the process-level reservation
    // still prevents a replacement pump while the old thread may be running.
    let mut input_stop_clean = true;
    if let Some(bridge) = state.input_bridge.as_mut() {
        let _ = bridge.handle.request_stop();
        // The loop has ended so no fresh snapshots enter after stop. The pump
        // closes admission and releases hooks without dispatching queued work.
        let report = bridge.handle.wait_stopped(crate::input_pump::STOP_WAIT);
        let mut event = serde_json::json!({"event":"input-stop","clean": report.clean, "unhook_ok": report.unhook_ok});
        if !report.clean {
            event["note"] = serde_json::json!(
                "input stop unclean; process exit releases hooks; no overlapping worker started"
            );
        }
        log_json_at(&log_path, event);
        input_stop_clean = report.clean;
    }
    // Renderer graceful stop: the loop has ended so no fresh frames enter
    // after stop (admission closed and epochs invalidated above), the
    // renderer destroys every carrier on its creator thread with verified
    // readbacks, and acks with a bounded wait. A missing ack returns an
    // unclean report without claiming clean and retains `Stopping` so no
    // overlapping worker can start; the existing recovery lifecycle is
    // untouched. Published overlay identities clear only after verified
    // destruction plus exit (never on timeout), so a recycled HWND value
    // can never linger as a stale exclusion while a live carrier may still
    // own it. Never waits indefinitely or holds owner locks while joining.
    let mut render_stop_clean = true;
    if let Some(bridge) = state.render_bridge.as_mut() {
        let _ = bridge.handle.as_mut().map(|handle| handle.request_stop());
        let report = bridge
            .handle
            .as_mut()
            .map(|handle| handle.wait_stopped(RENDER_STOP_WAIT))
            .unwrap_or(crate::render_handoff::RenderStopReport {
                destroyed_ok: false,
                clean: true,
            });
        let mut event = serde_json::json!({"event":"render-stop","clean": report.clean, "destroyed_ok": report.destroyed_ok});
        if !report.clean {
            event["note"] = serde_json::json!(
                "render stop unclean; process exit destroys carriers; no overlapping worker started"
            );
        }
        log_json_at(&log_path, event);
        render_stop_clean = report.clean;
        if report.clean {
            bridge.channels.clear_overlay_ids();
        }
    }
    if let Some(mut snap) = snap_hook {
        // Honest release accounting: process exit releases the hook in any
        // case, but success is recorded, never fabricated.
        let ok = crate::snapkey::sys::uninstall(&mut snap);
        let mut release = serde_json::json!({"event":"snap-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    if let Some(mut mouse) = windrag_hook {
        let ok = crate::win_mouse::sys::uninstall(&mut mouse);
        let mut release = serde_json::json!({"event":"windrag-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    // Owner tray teardown: graceful stop removes the icon (`NIM_DELETE`
    // through RAII `Drop`); a crash ghost is pruned by the next startup or
    // independent restore through the stable GUID. The explicit end line
    // marks graceful stop only.
    if let Some(tray) = tray {
        drop(tray);
        log_json_at(&log_path, serde_json::json!({"event":"tray-end"}));
    }
    // Owned overlay teardown: explicit destroy on graceful stop (process exit
    // destroys it implicitly after a crash, so no residue either way). With
    // a render bridge the renderer destroyed its carriers above and the
    // `*-end` lines report the last acknowledged applied state; otherwise
    // the same-thread carriers destroy inline as before.
    let border_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Border)
    } else {
        state.border_overlay.snapshot()
    };
    state.border_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"active-border-end","overlay": border_snapshot}),
    );
    let underlay_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Underlay)
    } else {
        state.underlay_overlay.snapshot()
    };
    state.underlay_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"group-underlay-end","overlay": underlay_snapshot}),
    );
    // Drop-preview teardown: explicit nonactivating destroy on graceful
    // stop (process exit destroys it implicitly after a crash, so no
    // residue either way). No ledger, no registry, no hidden windows.
    let preview_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Preview)
    } else {
        state.preview_overlay.snapshot()
    };
    state.preview_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"drag-preview-end","overlay": preview_snapshot}),
    );
    // Graceful stop restores only project-raised topmost bands with no frame
    // change, under the same held ownership gates as dispatch. Geometry is
    // left in place; a crash leaves every frame where it is, and restart
    // resets this runtime-local preimage with the Engine.
    let mut float_topmost_restored = 0u32;
    let mut float_topmost_skipped = 0u32;
    let float_preimages: Vec<(crate::workspace::WindowKey, bool)> = state
        .float_topmost_prev
        .iter()
        .map(|(key, prior)| (key.clone(), *prior))
        .collect();
    for (key, prior) in float_preimages {
        let Some(held) = hold_band_target(&state, me, &key) else {
            float_topmost_skipped += 1;
            continue;
        };
        let current = read_topmost_now(key.hwnd);
        if !crate::tiling::float_topmost_restore_needed(prior, current) {
            continue;
        }
        let ok = set_topmost_band(key.hwnd, prior)
            && read_topmost_now(key.hwnd) == prior
            && pid_current(key.hwnd, key.pid);
        let _ = &held;
        if state.audit_path.is_some() {
            audit_json(
                &state,
                serde_json::json!({
                    "event": "proof-topmost",
                    "op": "stop",
                    "restored": ok,
                }),
            );
        }
        if ok {
            float_topmost_restored += 1;
        } else {
            float_topmost_skipped += 1;
        }
    }
    state.float_topmost_prev.clear();
    state.floated.clear();
    state.float_rects.clear();
    if float_topmost_restored > 0 || float_topmost_skipped > 0 {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "float-topmost-restore",
                "restored": float_topmost_restored,
                "skipped": float_topmost_skipped,
            }),
        );
    }
    log_json_at(
        &log_path,
        serde_json::json!({"event":"tile-end","ticks":state.tick}),
    );
    // Truthful stop accounting: a missing input-pump or renderer
    // acknowledgement never claims clean. Teardown above still ran (tray,
    // overlays, restore), so this only changes the reported outcome, never
    // the recovery lifecycle.
    if !input_stop_clean {
        return Err(err(
            "error: input pump stop unacknowledged; hooks released by process exit",
        ));
    }
    if !render_stop_clean {
        return Err(err(
            "error: render thread stop unacknowledged; carriers destroyed by process exit",
        ));
    }
    result
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
