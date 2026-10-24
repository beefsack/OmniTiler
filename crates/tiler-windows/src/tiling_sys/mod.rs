#![forbid(unsafe_code)]
//! Native tiling-only runtime (`cfg(windows)` only).
//!
//! One product loop: PMv2 awareness verified first, full `EnumWindows`
//! enumeration per tick, the retained core world engine as layout authority,
//! `SetWindowPos` actuation with independent readback, and out-of-context
//! `SetWinEventHook` callbacks delivered on the loop thread itself (the loop
//! pumps messages; no hook thread, no join). Stop leaves all geometry
//! untouched; there is no per-window geometry ledger and nothing is ever
//! hidden.
//!
//! Production log vocabulary is structured JSON with opaque window tokens
//! only: no HWNDs, pids, process creation strings, SIDs, paths, or titles.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, OwnedHandle};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tiler_core::boundary::CoreCommand;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::size_hints::WindowSizeHints;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, MINMAXINFO, SW_MAXIMIZE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, WPF_ASYNCWINDOWPLACEMENT, WS_CAPTION,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_THICKFRAME,
};

use self::action_context::{ActionCtx, ActionLine, ActionTimings, SelectEffect, SendEffect};
#[cfg(test)]
use self::admission::admission_clear_planned;
use self::admission::{
    clear_maximize_at_admission, engine_is_float, fixed_origin_provenance, fixed_predicate_for,
    overlay_refusal_for,
};
#[cfg(test)]
use self::assignments::MAX_HIDE_DIAG_PER_SELECT;
use self::assignments::{ensure_workspace_assignments, workspace_close_cleanup};
use self::chord_resolution::{
    ChordObserved, ChordTarget, chord_overlay_refusal, observe_chord_intent,
    orientation_overlay_refusal, resolve_chord_target, settle_chord_reject,
};
use self::cli_directional::poll_directional_cli_request;
use self::cli_migrate_workspace::poll_migrate_workspace_cli_request;
use self::cli_resize::poll_resize_cli_request;
use self::cli_send_output::poll_send_output_cli_request;
use self::cli_workspace::poll_workspace_cli_request;
pub use self::commands::{
    cmd_capture, cmd_directional, cmd_inventory, cmd_migrate_workspace, cmd_resize,
    cmd_send_output, cmd_tile, cmd_workspace, load_normal_tile_base,
};
use self::diag::{WindowDiagSample, diag_identity_broke, diag_identity_rebound, drop_diag_sample};
use self::dir_watch::DirWatch;
use self::directional_resolution::{drop_arrival_verified, output_for_point, output_for_rect};
pub use self::dispatch::{LaunchBackend, SystemLaunchBackend};
use self::dispatch::{
    dispatch_float_intent, dispatch_launch_intent, dispatch_orientation_intent,
    dispatch_sticky_intent, dispatch_workspace_tiling_intent,
};
use self::drag_preview::refresh_drag_preview;
use self::first_run::{FirstRunPending, resolve_first_run};
pub use self::first_run::{FirstRunResolution, resolve_first_run_outcome};
use self::float_toggle::{
    NativeUnfloatClear, UnfloatClearEffects, apply_float_from_tiled, apply_unfloat_to_tiled,
    clear_float_mark, clear_sticky_mark, restore_raised_band, unfloat_maximize_clear,
};
use self::focus::{hide_border, note_focus_advance, publish_managed, refresh_active_border};
pub(crate) use self::focus_actuation::owner_snap_dropped;
use self::focus_actuation::{
    born_maximized_keep_overlay_for, overlay_expected, same_axis_move_for,
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
use self::geometry::{ApplySummary, audit_json};
use self::gesture_tick::gesture_tick;
use self::group_underlay::{
    chord_held_now, classify_move_size_start, hide_preview, hide_underlay, live_outer_rect,
    refresh_group_underlay,
};
use self::hints::{
    HintCx, HintFallbackUse, HintOutcome, LastGoodHint, hint_now, invalidate_last_good,
    note_last_good, timeout_candidate,
};
#[cfg(test)]
use self::hints::{clear_test_hints, inject_hint_now, inject_test_hints};
use self::history::{log_workspace_action, workspace_history_tick, workspace_log};
use self::input::{
    InputBridge, log_input_faults, offer_input_control, offer_input_snapshot, poll_input_ack,
};
pub use self::inspection::{
    cmd_border_inspect, cmd_children, cmd_inspect, cmd_preview_inspect, cmd_underlay_inspect,
};
use self::inspection::{hosted_gate_allows, medium_caller};
use self::keyboard_resize::dispatch_resize_intent;
use self::keyboard_tick::keyboard_tick;
use self::migration::{workspace_migrate_one, workspace_migrate_tick};
use self::minmax::{FreshHint, fixed_birth_for_slotless, hidden_hint_for, min_hint_for};
pub(crate) use self::monitors::ensure_pm_v2;
use self::monitors::{MonitorArea, all_monitors, monitor_fulls};
pub(crate) use self::monitors::{mark_monitor_dirty, take_monitor_dirty};
use self::native_drag::{capture_gesture_start, esc_edge_cancels};
use self::observation::{cursor_pos, enumerate_hwnds};
#[cfg(test)]
use self::observe::WaitWake;
use self::observe::{
    HookEvent, InputWaitSplit, LoopEventSink, ObservedWindow, OwnerWaitCounts, RenderWaitSplit,
    class_of, drain_owner_messages, foreground_lifetime, hook_edges_pending, hook_queue,
    observe_window, pump_wait_with_intent, pump_waits_line, verify_proof_owned, window_identity,
};
pub use self::observe::{KnownIdentity, ObserveFailure};
use self::output_sync::{
    foreground_elevated, foreground_pager_elevated_detailed, sync_monitor_outputs,
    workspace_maintenance,
};
#[cfg(test)]
use self::output_sync::{foreground_elevated_detailed, foreground_pager_elevated};
use self::owner_work::OwnerWorkGuard;
use self::pending_drop::{PendingDrop, queue_pending_drop, service_pending_drop};
#[cfg(test)]
use self::pending_seams::{
    INJECTED_ELEV_PROBE, INJECTED_ELEVATED, INJECTED_OBSERVE, INJECTED_STOP, INJECTED_SUSPEND,
    INJECTED_ZORDER, clear_pending_seams, inject_pending_elev_probe, inject_pending_elevated,
    inject_pending_fg_class, inject_pending_foreground, inject_pending_observe,
    inject_pending_parents, inject_pending_stop, inject_pending_suspend, inject_pending_tag,
    inject_pending_zorder,
};
use self::pending_seams::{
    live_cloaked, live_fg_class, live_foreground_window, live_is_iconic, live_is_visible,
    live_is_window, live_member_tag, live_parent_of, live_window_pid,
};
use self::preview::{PreviewBound, PreviewStartBinding};
pub use self::probes::live_overlay_flags;
use self::probes::{
    WriteTarget, is_zoomed_now, pid_current, restore_wake_probe, restore_zoom_placement,
    revalidate_target, toggle_zoom_async,
};
pub use self::proof_commands::{
    cmd_hide_proof, cmd_shortcut_proof, cmd_tile_proof, cmd_workspace_proof,
};
use self::reconcile::{reconcile_limited_tick, reconcile_tick};
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
    drop_member_state, hold_band_target, hold_marker_target, hydrate_tile_override_marker,
    install_float_mark_held, install_sticky_mark_held, install_tile_override_mark_held,
    marker_tag_pid_fresh, prune_float_state, remove_tile_override_mark_best_effort,
    retain_float_focus_validated,
};
use self::run_loop::run_tile_loop;
use self::run_loop_intake::{Intake, pump_intake};
use self::run_loop_tick::{TickFlow, run_active_tick};
use self::run_loop_wait::{
    DEGRADED_POLL_MS, MONITOR_FALLBACK_SECS, OWNER_WAIT_SOURCE_ORDER, OwnerDeadlines,
    OwnerWaitDiag, OwnerWaitSource, TRACE_REPORT_SECS, drain_hook_edges, gesture_sampling_open,
    loop_exit_cause, owner_deadline_source, pump_waits_snapshots, tray_staged_pending,
};
use self::selection::workspace_do_select;
use self::send_preflight::SendTarget;
use self::sends::workspace_do_send;
use self::settings::{
    TileRun, is_limited_fullscreen, poll_live_settings, suspend_limited, suspend_read,
    suspend_read_inner,
};
use self::shell::{exe_file_name, is_border_shell_popup, is_shell_class, rect_from_win};
use self::trace::{log_json_at, log_mono_ms, refresh_border_timed};
#[cfg(test)]
use self::visibility_diag::{
    REHIDE_DIAG_MAX_ENTRIES, REVEAL_DIAG_PER_SELECT, clear_visibility, inject_visibility,
};
use self::visibility_diag::{
    VisibilityDiagnostics, drop_visibility_sample, flush_exceptions, flush_rehide_diag,
    flush_visibility, note_rehide_diag, rehide_err_outcome, rehide_retired_detail,
    reveal_err_outcome, reveal_outcome, stage_exceptions, stage_visibility,
};
#[cfg(test)]
use self::visibility_diag::{note_hide_diag, note_reveal_diag, omission_summary, reveal_log_line};
use self::workspace_mode::read_topmost_now;
use self::workspace_mode::set_topmost_band;
use self::workspace_mode::toggle_workspace_tiling;
use self::workspace_mode::workspace_mode_known;
use self::workspace_mode::workspace_mode_tiled;
use self::workspace_navigation::workspace_tick;
use self::workspace_send_tick::{
    workspace_output_send_one, workspace_output_send_tick, workspace_relative_send_tick,
};
use crate::active_border::{ActiveBorderOptions, scale_style, scale_to_physical};
use crate::active_border_sys::{
    BorderOverlay, OverlayOutcome, PreviewOverlay, UnderlayOverlay, lowest_in_z,
};
use crate::group_underlay::{
    GroupUnderlayOptions, MoveSizeKind, aggregate_chord_keys, underlay_geometry_debug,
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
    SnapOp, SnapOrigin, VK_LSHIFT, VK_LWIN, VK_RSHIFT, VK_RWIN, VK_SHIFT, WorkspaceHistoryOp,
    WorkspaceOp, direction_name, resolve_origin,
};
use crate::storage::LedgerStore;
use crate::tiling::{
    AllowEntry, CaptureOptions, ChildrenOptions, DIRECTIONAL_REQUEST_FILE, DirectionalOptions,
    FrameInsets, HideProofOptions, INNER_GAP, InspectOptions, MIGRATE_WORKSPACE_REQUEST_FILE,
    MigrateWorkspaceOptions, OUTER_GAP, OWNER_ID, ObservedTarget, ObservedTargetRef,
    RESIZE_REQUEST_FILE, ResizeOptions, SEND_OUTPUT_REQUEST_FILE, ScopeHostChild,
    SendOutputOptions, SkipReason, StatelessVerdict, TileOptions, TileProofOptions, TokenMap,
    WindowFacts, WorkspaceAction, WorkspaceOptions, allow_match, allowlist_digest, classify,
    classify_focus, fullscreen_toggle_decision, hosted_child_allows, inspect_stateless_verdict,
    is_borderless_fullscreen, parse_allowlist, parse_directional_request,
    parse_migrate_workspace_request, parse_resize_request, parse_send_output_request,
    parse_workspace_request, scope_allows, scope_exe_basename, tiling_domain_bounds_with,
};
use tiler_core::gesture::preview_prior_desc;
type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

const TICK_POLL_MS: u32 = 100;

/// Unconfirmed workspace-mode toggle releases queue keyed by domain with
/// latest-intent-wins so a stale float can never fire at a retiled layout,
/// dropped on confirmed toggles, stale once tiled, and retried only on
/// topology edges with current observed geometry (never polled).
struct TileLoop {
    /// Single runtime WorldEngine owner (stage 2b, active): all
    /// lifecycle/history/tiling/scope/select/send/migration policy routes
    /// through `world`. The legacy `Engine`/`ManagedWorkspaces`/
    /// `WorkspaceScopeOwner` policy owners are deleted: core logical queries
    /// (`logical_ring`/`logical_view`/`logical_members`/`adapter_homing`)
    /// are the sole routing authority, read-only. Native caches below retain
    /// only proven applied homes/views/claims plus output native-wire ids.
    world: world_owner::WorldOwner,
    tokens: TokenMap,
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
    /// Stashed terminal gesture ingress (stage 4a): a release/cancel with
    /// its END-captured cursor, set by the END path before the committing
    /// observation tick and cleared after it returns. The tick's capture
    /// prefers this over deriving an update from the open tables. Never a
    /// second route: the END path dispatches no drag command of its own.
    gesture_override: Option<tiler_core::gesture::GestureIngress>,
    /// START-time identity snapshot per open gesture (pid/creation from a
    /// bounded owner-side probe plus the live visible-membership lifetime
    /// tag). Settle requires the fresh window to still match this snapshot in
    /// addition to the stored member, so a reuse between START and END fails
    /// closed even when HWND/PID/creation still agree. Absent entries fall
    /// back to the stored-member check only. The tag is the member property
    /// (`OmniTilerMember`), never the owned-helper property (which is
    /// empty for ordinary windows and would refuse every ordinary gesture).
    gesture_start_key: HashMap<u64, crate::native_guards::WindowKey>,
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
    /// True while the current suspension is the limited unmanageable-
    /// fullscreen lane (geometry for other windows continues). False for
    /// full suspension and when active. Lets the resume/transition logs
    /// name the mode that is ending without re-reading the foreground.
    suspended_limited: bool,
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
    /// Native top-to-bottom stacking snapshot from the latest observation,
    /// in the single enumeration sequence (first is topmost). The uniform
    /// presentation ingress maps it to opaque lifetimes for the pager
    /// order; frameless members keep their retained slot core-side.
    last_native_order: Vec<crate::native_guards::WindowKey>,
    /// Mechanical device-to-wire-output map (native device keys to opaque
    /// `oN` ids). Synced from live monitor areas; native-wire ids only,
    /// never rings/views/tiling/history policy.
    output_tokens: HashMap<String, String>,
    /// Adapter-retained last applied logical homing per Engine token (from
    /// the last applied reply rows). Receipts prove this commitment with
    /// fresh evidence; the core remains the sole homing authority.
    applied_homing: HashMap<String, (String, String)>,
    /// Adapter-retained last applied logical views per output (from the last
    /// applied reply outputs). View receipts prove this commitment.
    applied_views: HashMap<String, String>,
    /// Stable session tokens per member key. The token (not the ephemeral
    /// product nonce) is the Engine identity; the nonce lives only in
    /// `hidden_claims` plus the ledger while hidden.
    member_tokens: std::collections::BTreeMap<crate::native_guards::WindowKey, String>,
    /// Last-known rectangles per member token (visible reads plus hidden
    /// snapshots) so Engine source/target observations stay complete.
    member_rects: HashMap<String, Rect>,
    /// Known hidden product claims by stable member key. Retained
    /// independently of the `IsWindowVisible` observation gate so hidden
    /// windows are never dropped; the nonce leaves only after verified
    /// reveal or release. Keyed by the full `(hwnd, pid, creation)` member
    /// identity, never HWND alone, so a recycled HWND never matches.
    hidden_claims: std::collections::BTreeMap<crate::native_guards::WindowKey, HiddenRecord>,
    /// Stored full process identity per member key. Hide effects bind to
    /// this stored identity and refuse recycled HWNDs with no writes.
    member_identity: std::collections::BTreeMap<crate::native_guards::WindowKey, ProcessIdentity>,
    /// Stored visible-membership lifetime tag per member key, stamped at
    /// admission onto the window itself. Same-process HWND reuse starts
    /// without it, so every effect re-verifies live-against-stored before
    /// trusting HWND/PID/creation. Never serialized, never trusted across
    /// runs, never removed (the property dies with its window).
    member_tags: std::collections::BTreeMap<crate::native_guards::WindowKey, String>,
    /// Workspace scope mode adopted at owner startup (restart-adopted).
    /// Later live edits never adopt: the core reads the saved mode from
    /// every report ingress, and drift only logs `restart-required`.
    scope_adopted: tiler_core::workspace_scope::WorkspaceMode,
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
    /// Last monitor-topology fingerprint for the secondary-taskbar gate.
    /// The primary bar is re-read every pump; secondaries revalidate only
    /// on the stale-anchor flag (validated taskbar-subtree edge, rediscovery
    /// broadcast, or topology change), never while suspended. They only feed
    /// refusal logging, never positions.
    pager_areas_fp: u64,
    /// Cached primary taskbar facts from the last pump (fresh per wake) and
    /// cached secondaries from the last event-driven revalidate.
    pager_primary: Option<crate::pager::TaskbarFacts>,
    pager_secondaries: Vec<crate::pager::TaskbarFacts>,
    /// Taskbar-anchor invalidation: set by the hook-edge drain for
    /// pid-scoped Explorer window edges, by the native rediscovery
    /// broadcast, or by a monitor-topology change; cleared by the next
    /// secondary revalidation. The gate never inspects the cached census,
    /// so empty, unproven, and proven-but-churned anchors all revalidate
    /// on genuine Explorer change instead of a polling loop.
    pager_taskbar_dirty: bool,
    /// Cached Explorer pid behind the installed pid-scoped taskbar hooks
    /// (`None` while no scope is installed). Refreshed only on the
    /// rediscovery broadcast, never periodically; identity only, never
    /// geometry.
    pager_hook_pid: Option<u32>,
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
    /// `pager-suppress` logging. Reason strings only; read by the
    /// maintenance scheduler, so the diagnostic signature must never land
    /// here.
    pager_suppress_last: Option<String>,
    /// Last full pager-gate diagnostic signature (`None` until the first
    /// pump): change-only `pager-gate` logging, including the first allowed
    /// baseline and allowed-to-allowed fact changes. Independent of
    /// `pager_suppress_last`; taskbar-fact resets never touch it.
    pager_gate_last: Option<String>,
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
    born_keep_overlay: BTreeSet<crate::native_guards::WindowKey>,
    /// R-MAX-03 retile latch: exact fresh lifetimes bound at floating-first
    /// slotless admission. A floating-to-tiled workspace admission keeps the
    /// native maximize over the reserved tile slot with no one-shot clear,
    /// independent of `core.born_maximized` (only born R-MAX-06 reads that
    /// setting). Keyed by full member identity, never HWND alone; pruned
    /// with membership like `born_keep_overlay`.
    retile_keep_overlay: BTreeSet<crate::native_guards::WindowKey>,
    /// Born-fullscreen hold (KDE initial-fullscreen-hold parity): first-seen
    /// fullscreen windows without a retained tile slot, tracked slotless as a
    /// planner-only exception until their first exit. Keyed by full member
    /// identity, never HWND alone. A window already seen non-fullscreen in
    /// this lifetime never enters.
    born_fullscreen: BTreeSet<crate::native_guards::WindowKey>,
    /// Lifetime-known non-fullscreen member identities: every admitted or
    /// observed-eligible member lands here, so a later fullscreen transition
    /// is a managed overlay, never a born hold again. Pruned with the
    /// enumeration like the hold set.
    seen_nonfullscreen: BTreeSet<crate::native_guards::WindowKey>,
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
    /// Their world facts minimize so capture converges the leaf,
    /// reflows survivors, resolves MRU-or-none focus, and stores the
    /// `MinimizedSlot`. Populated in `observe`, consumed in capture;
    /// cleared on every real observation pass.
    /// Session-only, never persisted.
    minimized_omitted: HashSet<crate::native_guards::WindowKey>,
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
    hint_last_good: HashMap<crate::native_guards::WindowKey, LastGoodHint>,
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
    /// Fair emission order: per-token sequence of last emission (0/absent
    /// means never emitted). Never-emitted and oldest-emitted tokens emit
    /// first so a flapping head cannot starve the tail. Pruned with
    /// membership.
    diag_emit_seq: HashMap<String, u64>,
    /// Tokens merged by throttled rounds since the last emission (unique:
    /// a token counts once no matter how many rounds it waits through).
    /// Reported as `coalesced`, cleared on emit, pruned with membership.
    diag_coalesced: HashSet<String>,
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
    /// Independent pager focused-miniature highlight for this run (default
    /// system accent with the configured fallback). Live file edits adopt
    /// through the settings lane; proof owners keep the default.
    pager_highlight: crate::pager_highlight::PagerHighlightStyle,
    /// Process-owned border overlay plus the last logged border signature
    /// (visibility/rect/style/target/colour/source) for change-only logging.
    border_overlay: BorderOverlay,
    border_last: Option<String>,
    /// Last observed shared accent broadcast version. The renderer (or a
    /// loop-thread window) bumps the version on accent broadcasts while the
    /// owner sleeps; a move observed before the ack-only early continue
    /// promotes to a full refresh turn so `refresh_active_border` reposts
    /// from fresh observation. Never fakes an ack or replays a frame.
    accent_seen: u64,
    /// Single shared versioned accent cache for this scope: the owner
    /// resolves once per broadcast version and the border plus the pager
    /// both paint from the same snapshot (plain data, never a second
    /// native call per consumer).
    accent_shared: crate::active_border::AccentCache,
    /// Owner-thread-only WinRT accent source with a scope-lifetime
    /// apartment. Lazy: construction performs no WinRT call, so offline
    /// fixtures never touch WinRT; the first resolve initializes and
    /// shutdown (at scope end) balances it.
    accent_owner: crate::ffi::OwnerAccentSource,
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
    /// movement). Cleared on END-settle/removal like `gesture_before`.
    gesture_start_cursor: HashMap<u64, (i32, i32)>,
    /// Gesture ingress phase already reported per open HWND: the first
    /// observation tick carrying an open gesture reports `Started` (with
    /// the down-point cursor above), later ticks report `Updated`. Gives
    /// the core an explicit new-gesture signal (START truth plus
    /// at-most-once supersession) without a second report. Cleared on
    /// END-settle/removal like `gesture_before`.
    gesture_started_sent: HashSet<u64>,
    /// Stable adapter gesture id per open native gesture HWND: nonzero,
    /// allocated once at native START drain, retained across coalesced
    /// START/END through settle and the bridged-hide wait. Every ingress
    /// for the gesture (Started, Updated, terminal, cancel) carries it,
    /// so the core orders sessions and at-most-once terminals by id
    /// alone. Cleared on END-settle/removal like `gesture_before`.
    gesture_seq: HashMap<u64, u64>,
    /// Next stable gesture id to allocate (monotonic within the process,
    /// never zero, never reused). Reset is unnecessary: the core resets
    /// its consumed watermark on adapter incarnation change, and every
    /// id allocated after that still orders above it.
    next_gesture_seq: u64,
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
    float_topmost_prev: std::collections::BTreeMap<crate::native_guards::WindowKey, bool>,
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
    floated: BTreeSet<crate::native_guards::WindowKey>,
    /// Sticky-float members by stable member key to their pre-sticky float
    /// state (`true` when the window was already a normal float before
    /// sticky-on). Single runtime map, never redundant sets. Sticky rides the
    /// Engine as a slotless float plus this map plus a window-lifetime native
    /// marker; the marker (not this map) survives owner restarts.
    sticky: std::collections::BTreeMap<crate::native_guards::WindowKey, bool>,
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
    restart_reject_hold: BTreeSet<crate::native_guards::WindowKey>,
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
    fixed_tile_overrides: BTreeSet<crate::native_guards::WindowKey>,
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
    fixed_origin_overrides: BTreeSet<crate::native_guards::WindowKey>,
    /// Adapter-asserted automatic fixed-float origins by stable member key
    /// (R-SPC-04 D4): first-seen maximized windows whose fresh tracks are
    /// fixed under the live predicate. They admit slotless and ride floating
    /// beneath native maximize with no clear and no reserved tile slot; the
    /// stamped origin lets the Engine mark them automatic. Lifetime-bound,
    /// pruned with membership; cleared on explicit unfloat (which records an
    /// override instead) and re-float.
    fixed_auto_seen: BTreeSet<crate::native_guards::WindowKey>,
    /// First born-fullscreen exits by stable member key (R-SPC-04 D5): the
    /// exit tick classifies with current hints and predicate before any
    /// maximize-clear/restore/slot side effect. One-shot: the next
    /// maximize-clear pass consumes the entry (fixed exits float automatic
    /// and skip the clear via the Engine mark; non-fixed exits tile and take
    /// the normal one-shot clear). Pruned with membership.
    born_first_exit: BTreeSet<crate::native_guards::WindowKey>,
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
    /// Event-driven cursor-motion tap for focus-follows-cursor: armed only
    /// from live settings on active (non-game) ticks, physically off under
    /// fullscreen suspension and on proof paths. Never reads the cursor
    /// itself; motion only wakes the loop, sampling stays per-turn.
    cursor_wake: cursor_wake::CursorWake,
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
/// would inflate generations on every stalled tick).
pub(crate) fn plan_snapshot_offer(
    last: Option<&SharedSnapshot>,
    candidate: &crate::input_handoff::InputSnapshot,
    retry_gen: Option<u64>,
) -> SnapshotOffer {
    if let Some(generation) = retry_gen {
        return SnapshotOffer::Retry(generation);
    }
    if let Some(last) = last
        && snapshot_content_eq(last, candidate)
    {
        return SnapshotOffer::Skip;
    }
    SnapshotOffer::Fresh
}

/// Armed prompt-restore wake: the restored member by full identity plus the
/// Engine token, so a recycled HWND can never count as completion proof.
struct RestoreWake {
    key: crate::native_guards::WindowKey,
    token: String,
    until: Instant,
}

#[cfg(test)]
#[path = "tests_esc.rs"]
mod esc_sequence_tests;

#[cfg(test)]
#[path = "tests_input_snapshot.rs"]
mod input_snapshot_offer_tests;

mod scope_sends;

mod scope_hotplug;

mod monitors;
mod shell;
mod tray_select;

mod observe;

mod hints;

mod pending_seams;

mod minmax;

mod trace;

mod owner_work;

mod preview;

mod input;

mod render_bridge;

mod render_ack;

mod accent_owner;

mod retained;

mod focus;

mod group_underlay;

mod drag_preview;

mod probes;

mod fullscreen;

mod admission;

mod capacity_floats;

mod rows;

// W1 world capture (runtime owner path): pure source-first observation
// capture over the real owner tables. Sequencing/actuation below drive
// the single WorldEngine owner; legacy Engine/ManagedWorkspaces policy
// owners are removed by the 2b cutover.
mod world_capture;

// W1 sequencing boundary (runtime): the in-process adapter
// envelope/ordering/ack over the capture above, sequenced through the
// Windows-profile WorldEngine owned by TileLoop.
mod world_sequencer;

// W2 host-diff actuator (runtime): the host diff over typed
// replies. Offline facade plus production NativeOps share the same
// authority/freshness/proof rules.
mod world_diff;
// Production native actuator over the diff: the real NativeOps
// plus truthful applied-receipt capture, wired through the tick.
mod world_native;
// Truthful applied-receipt capture for the world route: the
// capture-facing `observed_workspace`/`observed_views` projections over
// fresh visibility plus the owner tables (actuator stays in `world_native`).
mod world_native_receipts;
// Single runtime WorldEngine owner (stage 2b cutover): the in-process
// holder over the sequencer above. All lifecycle/history/tiling/scope/
// select/send/migration routes dispatch through it.
mod world_owner;

// Single active world tick (stage 2b, wired): mechanical admission plus
// capture/sequence/deliver/apply/ack through the owner above. The sole
// production owner path; legacy policy routes are deleted.
mod world_tick;

// World-backed read helpers for the single-owner cutover: every routing
// query over the sequenced core plus the mechanical adapter caches.
// No policy, no mutation beyond stable wire-id minting.
mod world_reads;

// Restored offline safety regressions over capture/sequencer/diff.
#[cfg(test)]
mod world_diff_tests;

// Shared frame-convergence through the production seam: the same fixture
// sections the core runners consume, with every fact produced by the real
// native seam. Split from `world_diff_tests`; rig helpers are imported.
#[cfg(test)]
mod world_diff_shared_convergence_tests;

// Native-free route smoke test: capture shape plus sequencer plus offline
// facade actuation with logical view echoes. Same W3 cutover gate.
#[cfg(test)]
mod world_route_tests;

// Stage-5 directional seam: float origins dispatch world commands and the
// core owns every logical decision (no adapter bypass); the adapter only
// captures truth and mechanically actuates the reply.
#[cfg(test)]
mod tests_directional_world;

// Stage 2c cursor-first boundary: capture/sequencer projection, production
// selects and sends under cursor/focus disagreement, explicit outputs,
// and the focused-fullscreen-game cursor guard.
#[cfg(test)]
mod tests_cursor_targeting;

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

mod diag;

mod visibility_diag;

mod restart;

mod assignments;

mod workspace_mode;

mod selection;

mod send_preflight;

#[cfg(test)]
#[path = "tests_hint_fallback.rs"]
mod hint_fallback_tests;

mod sends;

mod history;

mod action_context;

mod output_sync;

mod pager_host;

mod cli_directional;

mod cli_resize;

mod cli_migrate_workspace;

mod cli_send_output;

mod cli_workspace;

mod workspace_navigation;

mod workspace_send_tick;

mod migration;

mod native_drag;

mod pending_drop;

mod gesture_tick;

mod settings;

mod dir_watch;
mod run_loop;
mod run_loop_intake;
mod run_loop_tick;
mod run_loop_wait;

mod commands;

mod cursor_wake;

mod first_run;

mod proof_commands;

mod inspection;

mod observation;

#[cfg(test)]
#[path = "tests_same_axis.rs"]
mod same_axis_move_tests;

#[cfg(test)]
#[path = "tests_owned_exit.rs"]
mod owned_exit_tests;

#[cfg(test)]
#[path = "tests_float_send_gate.rs"]
mod float_send_gate_tests;

/// Select-route gap-repair regression (offline, native-free): the real
/// `workspace_do_select` reveal must keep `UpdateGaps` repair victims
/// instead of re-tiling them from stale pre-adopt rows.
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
#[path = "tests_rmax03.rs"]
mod rmax03_adapter_tests;

#[cfg(test)]
#[path = "tests_visibility_diag.rs"]
mod visibility_diag_tests;

#[cfg(test)]
#[path = "tests_restart.rs"]
mod restart_adoption_tests;

#[cfg(test)]
#[path = "tests_directional_cli.rs"]
mod directional_cli_tests;

#[cfg(test)]
#[path = "tests_send_output_cli.rs"]
mod send_output_cli_tests;

#[cfg(test)]
#[path = "tests_migrate_workspace_cli.rs"]
mod migrate_workspace_cli_tests;

#[cfg(test)]
#[path = "tests_intent_wake.rs"]
mod intent_wake_tests;

#[cfg(test)]
#[path = "tests_owner_wait.rs"]
mod owner_wait_tests;

#[cfg(test)]
#[path = "tests_launch.rs"]
mod launch_dispatch_tests;

#[cfg(test)]
/// Stage 3 combined transport/fault stress over the real production
/// primitives (no TileLoop, no threads, no native calls): the input
/// sender stall contract and the render latest-wins/safety
/// contract hold simultaneously, pinning the suspend-path dual handoff
/// (infallible input clear plus all-carrier render hides with no stale
/// show presentable afterwards).
#[path = "tests_stage3.rs"]
mod stage3_combined_tests;

#[cfg(test)]
#[path = "tests_limited_fullscreen.rs"]
mod limited_fullscreen_tests;

/// Pending-drop slot-machine regressions (offline, native-free): the bridged
/// `queue_pending_drop` / `service_pending_drop` finish path over the current
/// world route. See the module doc for the accepted-behavior map.
#[cfg(test)]
#[path = "tests_pending_drop.rs"]
mod pending_drop_tests;
