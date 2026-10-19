#![forbid(unsafe_code)]

//! Live native send backend: production evidence assembly over injectable
//! OS readers plus the single-use permit posts and the terminal reaper.
//!
//! Numbered send-follow 1..9 is armed; trailing send-0, stays, relative
//! sends, and outputs stay refused engine-side.
//!
//! Authority order is fixed: [`SendNativeReaders::engine_facts`] (current
//! [`EngineManagedFacts`](crate::runtime_core::EngineManagedFacts) from the
//! engine thread) first — a subject the engine no longer manages refuses
//! with zero further reads, even when every native leg would still match.
//! Native OS legs (joined topology, SLS membership, AX state, focus
//! oracles, CG ownership, dual hit tests, tagged HID posts, arrival
//! readback) supplement engine facts but never replace them. The per-post
//! ordinal/revision anchor refuses a reorder between the row read and the
//! chord post instead of sending to the new occupant. Releases bypass the
//! anchor and always attempt. [`spawn_terminal_reaper`] owns one
//! unfinished effect owner on a dedicated thread until its terminal
//! completion and only then releases the lane — never a join that blocks
//! the engine, AppKit/main, or any observation owner.

use std::sync::Arc;

use crate::desktop_shortcuts::ShortcutEntry;
use crate::mc_probe::ActionBudget;
use crate::runtime::RuntimeHandle;
use crate::runtime_types::NativeContext;
use crate::space_drag_transfer::{
    DragStep, PostCounts, candidate_grab_points, hit_test_input_grab,
};
use crate::synth::HidMouseKind;
use crate::workspace_select::SelectionSnapshot;
use crate::workspace_send::SendGates;
use crate::workspace_send_effect::{
    BlindRoot, ManagedSendTarget, SendFreshEvidence, TrailingPrepPins, TrailingPrepProof,
};
use crate::workspace_send_managed::{ManagedPostPermit, ManagedSendBackend, ManagedSendThread};

/// Joined fresh send topology from one read set: the single-display user
/// row, the fingerprint revision, and the derived native context. Joined at
/// once so the ordinal position of the pinned SID, the proof revision, and
/// the source/current homing legs cohere — never a row from one snapshot
/// fenced by another snapshot's revision.
#[derive(Debug, Clone)]
pub struct SendTopology {
    pub row: SelectionSnapshot,
    pub revision: u64,
    pub context: Arc<NativeContext>,
}

/// Fresh AX subject state over one pinned window: every leg `Option`
/// (unknown refuses, never guessed). Read with fresh create/use/drop AX
/// handles on the effect owner only — never on the engine, AppKit, or
/// per-app observation owners.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AxSubjectState {
    pub frame: Option<crate::ffi::AxFrame>,
    pub fullscreen: Option<bool>,
    pub minimized: Option<bool>,
}

/// Fresh CG grab context over one pinned window: the owned normal-layer
/// entry plus the full basic list for the input-reaching hit proof.
/// `None` from the reader when the owned entry is missing (unknown, never
/// guessed).
#[derive(Debug, Clone)]
pub struct GrabContext {
    pub owned: crate::ffi::CgWindow,
    pub all: Vec<crate::ffi::CgWindowBasic>,
}

/// Injectable native readers seam for the live managed-send backend. Every
/// method runs on the dedicated effect owner only. Production
/// ([`OsSendReaders`]) performs fresh OS reads plus tagged HID posts there;
/// tests script every leg. Single reads, never polls; `None`/`Err` legs mean
/// unknown and always refuse downstream — never guessed, never retried
/// here. Sequential reads are honest TOCTOU: proof, bundle, oracle, and
/// post are sequential live reads, not one atomic transaction.
pub trait SendNativeReaders: Send {
    /// Fresh session gates (plain booleans, no proof content).
    fn read_gates(&mut self) -> SendGates;
    /// Current engine-managed facts for one subject window: generation,
    /// homing source plus current actuation, retained fullscreen,
    /// classifier-excluded minimized lifetime, and the retained revision.
    /// `Managed` carries fresh engine authority; `Unmanaged` is a proven
    /// engine reply that the window is no longer managed; `NoReply` is an
    /// unproven read (bound elapsed, inbox closed, engine down). Both
    /// non-managed variants refuse without actuation — never guessed,
    /// never an admission-pin substitute. This is the managed-window
    /// authority: native OS legs below supplement it but never replace it.
    fn engine_facts(&mut self, window: &str) -> crate::runtime_core::EngineFactsRead;
    /// Joined fresh topology (row plus revision plus context from one read
    /// set — the ordinal/revision/homing coherence root). The display legs
    /// are latest-wins main-thread publications; rows, types, membership
    /// and current stay fresh native reads on this thread.
    fn fresh_topology(&mut self) -> Result<SendTopology, String>;
    /// Fresh native display-row count (product single-display rule).
    fn display_count(&mut self) -> Result<usize, String>;
    /// Fresh Desktop-N holder read by symbolic id (never a settings write).
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String>;
    /// Fresh SLS Space membership for one window id (`Err` = unknown).
    fn subject_spaces(&mut self, wid: u32) -> Result<Vec<u64>, String>;
    /// Fresh AX subject state (frame, fullscreen, minimized).
    fn subject_ax(&mut self, pid: u32, wid: u32) -> AxSubjectState;
    /// Fresh selected-focus identity (PID plus focused window, if any).
    fn selected_focus(&mut self) -> Option<(u32, Option<u32>)>;
    /// Fresh selected-oracle agreement on exactly `(pid, wid)`: SLPS
    /// front plus workspace frontmost plus direct per-app selected window
    /// with CG ownership (see [`crate::focus::read_agreement`]).
    fn focus_agrees(&mut self, pid: u32, wid: u32) -> bool;
    /// Fresh CG grab context for exactly `(pid, wid)` (`None` = unknown).
    fn grab_context(&mut self, pid: u32, wid: u32) -> Option<GrabContext>;
    /// Fresh positional AX hit-test detail at one point: the exact owner
    /// plus the hit-element `AXRole`/`AXSubrole`, all from the same
    /// element-at-position read (`None` = unknown, never guessed).
    fn ax_hit_detail(&mut self, x: f64, y: f64) -> Option<crate::ffi::AxHitDetail>;
    /// Fresh WindowServer hit-test window at one point (`None` = unknown).
    fn ws_hit_wid(&mut self, x: f64, y: f64) -> Option<u32>;
    /// One tagged HID mouse post (own-switch provenance, zero flags). Only the
    /// transfer executor's mouse vocabulary travels here; chord steps refuse.
    fn hid_mouse(&mut self, step: DragStep, point: (f64, f64)) -> Result<(), String>;
    /// One tagged HID key edge (own-switch provenance).
    fn hid_key(&mut self, keycode: u32, flags: u64, down: bool) -> Result<(), String>;
    /// Fresh current Space for the single display (`None` = unknown).
    fn read_current(&mut self) -> Option<u64>;
    /// Ordinary native select of one existing SID (stay return only). The
    /// production override runs the proven SID-accepting goto with
    /// exact-current readback on this thread; the default refuses honestly.
    fn select_space(&mut self, sid: u64) -> Result<(), String> {
        let _ = sid;
        Err("stay return unimplemented: no live select-back route".to_owned())
    }
}

/// Bound for one engine-managed-facts query from the effect owner: the
/// engine answers from retained truth over the control lane (strict FIFO
/// with observations). A late reply is unknown facts and refuses — never
/// waited out on the owner.
const ENGINE_FACTS_TIMEOUT_MS: u64 = 200;

/// Production native readers over the calling (effect-owner) thread: the
/// AppKit display legs arrive as latest-wins notification publications;
/// every other leg stays a fresh read here. Compiles on every host; no
/// new `unsafe`.
pub struct OsSendReaders {
    handle: RuntimeHandle,
    store: crate::desktop_shortcuts::LiveStore,
}

impl OsSendReaders {
    /// Production readers for one effect owner: the runtime handle serves
    /// the app-lifetime generation leg from lock-shared admission truth
    /// (brief lock only, never AX); the live store serves holder reads.
    #[must_use]
    pub fn live(handle: RuntimeHandle) -> Self {
        Self {
            handle,
            store: crate::desktop_shortcuts::LiveStore::new(),
        }
    }
}

impl SendNativeReaders for OsSendReaders {
    fn read_gates(&mut self) -> SendGates {
        SendGates {
            session_unlocked: matches!(
                crate::ffi::session_state().lock,
                crate::ffi::SessionLock::Unlocked
            ),
            secure_active: matches!(crate::ffi::secure_input_known(), Some(true)),
            lock_screen: crate::ffi::frontmost_app()
                .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
                .unwrap_or(true),
        }
    }
    fn engine_facts(&mut self, window: &str) -> crate::runtime_core::EngineFactsRead {
        self.handle.query_managed_facts_read(
            window,
            std::time::Duration::from_millis(ENGINE_FACTS_TIMEOUT_MS),
        )
    }
    fn fresh_topology(&mut self) -> Result<SendTopology, String> {
        // Notification-published display legs plus fresh SLS rows/types here.
        let published = self.handle.published_display_inputs().ok_or_else(|| {
            "send topology unreadable: display snapshot unpublished (no main-thread publish yet)"
                .to_owned()
        })?;
        let rows = crate::ffi::sls_display_rows()
            .map_err(|e| format!("send topology unreadable: space rows ({e})"))?;
        let revision =
            crate::topology::fingerprint_snapshot(&published.displays, &rows, published.separate);
        let snapshot = crate::topology::TopologySnapshot {
            displays: published.displays.clone(),
            rows,
            separate_spaces: published.separate,
            revision,
        };
        let revision = snapshot.revision;
        let (context, _) =
            NativeContext::derive(&snapshot, &|sid| crate::ffi::sls_space_type(sid).ok());
        let row = crate::workspace_select::user_row_from_rows(&snapshot.rows, 0)
            .map_err(|e| format!("send topology unreadable: user row ({e})"))?;
        Ok(SendTopology {
            row,
            revision,
            context: Arc::new(context),
        })
    }
    fn display_count(&mut self) -> Result<usize, String> {
        crate::workspace_select::fresh_display_count()
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        use crate::desktop_shortcuts::DesktopShortcutStore;
        self.store.read(symbolic_id)
    }
    fn subject_spaces(&mut self, wid: u32) -> Result<Vec<u64>, String> {
        crate::spaces::window_spaces(wid).map_err(|e| e.to_string())
    }
    fn subject_ax(&mut self, pid: u32, wid: u32) -> AxSubjectState {
        read_ax_subject_state(pid, wid)
    }
    fn selected_focus(&mut self) -> Option<(u32, Option<u32>)> {
        let evidence = crate::ffi::ax_selected_focus_evidence().ok()?;
        if evidence.pid <= 1 {
            return None;
        }
        u32::try_from(evidence.pid)
            .ok()
            .map(|pid| (pid, evidence.wid))
    }
    fn focus_agrees(&mut self, pid: u32, wid: u32) -> bool {
        crate::focus::read_agreement(pid, wid).is_ok()
    }
    fn grab_context(&mut self, pid: u32, wid: u32) -> Option<GrabContext> {
        let all = crate::ffi::cg_window_list_basic().ok()?;
        let entry = all.iter().find(|w| w.pid == pid && w.wid == wid)?;
        Some(GrabContext {
            owned: crate::ffi::CgWindow {
                wid: entry.wid,
                pid: entry.pid,
                layer: entry.layer,
                onscreen: entry.onscreen,
                bounds: entry.bounds,
                title: None,
            },
            all,
        })
    }
    fn ax_hit_detail(&mut self, x: f64, y: f64) -> Option<crate::ffi::AxHitDetail> {
        crate::ffi::ax_hit_test_detail(x, y).ok()
    }
    fn ws_hit_wid(&mut self, x: f64, y: f64) -> Option<u32> {
        crate::ffi::sls_window_at_point(x, y).ok()
    }
    fn hid_mouse(&mut self, step: DragStep, point: (f64, f64)) -> Result<(), String> {
        let Some(kind) = HidMouseKind::from_drag_step(step) else {
            return Err("refusing non-mouse step on the managed HID post".to_owned());
        };
        crate::synth::hid_mouse_post(
            kind,
            point,
            0,
            Some(crate::input_match::OWN_SWITCH_USER_DATA),
        )?;
        Ok(())
    }
    fn hid_key(&mut self, keycode: u32, flags: u64, down: bool) -> Result<(), String> {
        let code =
            u16::try_from(keycode).map_err(|_| "managed chord keycode unpostable".to_owned())?;
        crate::synth::hid_key_post(
            code,
            flags,
            down,
            Some(crate::input_match::OWN_SWITCH_USER_DATA),
        )?;
        Ok(())
    }
    fn read_current(&mut self) -> Option<u64> {
        self.fresh_topology().ok().map(|topo| topo.row.current)
    }
    fn select_space(&mut self, sid: u64) -> Result<(), String> {
        // Ordinary return select on the effect owner: the same pre-dispatch
        // gates as the native select path, then the proven SID-accepting
        // goto with exact-current readback over the main-thread-published
        // plain-data display legs. The SLS-to-CG join rides that pinned
        // `(cgid, managed UUID)` pair verified against every fresh SLS
        // snapshot by exact identifier - never an AppKit read, so this
        // thread neither touches nor blocks the main thread. Never a focus
        // guess or AX raise: the press addresses the MC Space list, never
        // an app window.
        if sid == 0 {
            return Err("stay return refused: source SID unpinned (zero)".to_owned());
        }
        match crate::ffi::session_state().lock {
            crate::ffi::SessionLock::Unlocked => {}
            _ => return Err("stay return refused: session not unlocked pre-dispatch".to_owned()),
        }
        if crate::ffi::secure_input_known() == Some(true) {
            return Err("stay return refused: Secure Input active pre-dispatch".to_owned());
        }
        if crate::ffi::frontmost_app()
            .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
            .unwrap_or(true)
        {
            return Err("stay return refused: lock screen pre-dispatch".to_owned());
        }
        // Effect-thread display join: the published plain-data legs must
        // name exactly one display (the product single-display rule), and
        // its managed UUID plus CG ID pin the selector below.
        let published = self.handle.published_display_inputs().ok_or_else(|| {
            "stay return refused: display snapshot unpublished (no main-thread publish yet)"
                .to_owned()
        })?;
        if published.displays.len() != 1 {
            return Err(format!(
                "stay return refused: single-display rule ({} published displays)",
                published.displays.len()
            ));
        }
        let pinned = &published.displays[0];
        let pinned_uuid = pinned
            .uuid
            .clone()
            .ok_or_else(|| "stay return refused: pinned display has no managed UUID".to_owned())?;
        if pinned.cg_id == 0 {
            return Err("stay return refused: pinned display has no CG display ID".to_owned());
        }
        let from = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("stay return refused: pre-select snapshot ({e})"))?;
        // Single-press budget matching the select drain bound; the goto
        // carries its own bounded settle poll (never replayed).
        let mut budget = ActionBudget::new(30);
        crate::mc_lifecycle::goto_sid_verified_pinned(
            &from,
            0,
            sid,
            pinned.cg_id,
            &pinned_uuid,
            &mut budget,
            false,
        )
        .map(|_| ())
        .map_err(|e| format!("stay return refused: select-back ({e})"))
    }
}

/// Fresh AX subject read with create/use/drop handles on the calling
/// (effect-owner) thread. Managed scope is the authority here — never the
/// fixture-owned [`crate::synth::OwnedPostTarget`] proof (fixture-only).
/// Every leg is `None` when unreadable. No new `unsafe`.
fn read_ax_subject_state(pid: u32, wid: u32) -> AxSubjectState {
    #[cfg(target_os = "macos")]
    let mut state = AxSubjectState::default();
    #[cfg(not(target_os = "macos"))]
    let state = AxSubjectState::default();
    #[cfg(target_os = "macos")]
    {
        use crate::ffi::{AxElement, ax_bool_attribute, ax_fullscreen_state, ax_get_frame};
        use crate::ffi::{ax_set_messaging_timeout, ax_window_id};
        if pid <= 1 || wid == 0 {
            return state;
        }
        let Ok(pid_i32) = i32::try_from(pid) else {
            return state;
        };
        let Some(app) = AxElement::application(pid_i32) else {
            return state;
        };
        let _ = ax_set_messaging_timeout(&app, 2.0);
        let Ok(windows) = app.windows() else {
            return state;
        };
        let Some(window) = windows.iter().find(|w| ax_window_id(w) == Some(wid)) else {
            return state;
        };
        let _ = ax_set_messaging_timeout(window, 2.0);
        state.frame = ax_get_frame(window).ok();
        state.fullscreen = ax_fullscreen_state(window).ok();
        state.minimized = ax_bool_attribute(window, "AXMinimized").ok();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, wid);
    }
    state
}

/// Blind evidence plus its root: every leg unknown, with the single
/// unreadable read named for refusal-only diagnosis. The proof verdict
/// stays [`SendProofRefusal::UnknownManaged`] for every root — the root
/// only names which read collapsed, never window content. No frame travels
/// with blind evidence (nothing was proven); the frame slot stays `None`.
fn blind_evidence(
    root: BlindRoot,
) -> (
    SendFreshEvidence,
    Option<BlindRoot>,
    Option<crate::ffi::AxFrame>,
) {
    (
        SendFreshEvidence {
            managed_current: None,
            fullscreen: None,
            minimized: None,
            live_generation: None,
            live_revision: None,
            live_source: None,
            source_current: None,
            focused_window: None,
            focus_agrees: None,
            geometry_ok: None,
            grab_point: None,
            ax_hit: None,
            ws_hit: None,
        },
        Some(root),
        None,
    )
}

/// Assemble one fresh-proof evidence set for a pinned target from the
/// readers' single reads. Authority order is fixed: the engine-managed
/// facts first (a subject the engine no longer manages — removed,
/// excluded, unmanaged, or an unreadable query — refuses with zero further
/// reads, even when every native leg would still match), then the joined
/// native topology, then the supplemental OS legs. Native OS reads
/// supplement engine facts but never replace them: fresh membership must
/// still name the engine-homed source Space, and fresh fullscreen or
/// minimized state can only add refusals, never permit what the engine
/// forbids. Revision coherence needs both legs: the retained engine
/// revision must still pin-match admission (else stale), and the fresh
/// native fingerprint must join the engine revision (else unknown — never
/// guessed). Returns the evidence plus the blind root (`Some` only when
/// the evidence collapsed to blind — fully readable evidence reports no
/// root). Grab selection tries the frame-derived titlebar candidates in
/// order and keeps the first point where BOTH the positional AX hit and
/// the WindowServer hit name the pinned target (input-reaching proof, not
/// rectangle stacking); no fully-clear point refuses with no new input
/// (log-only line, zero posts).
fn assemble_evidence<R: SendNativeReaders>(
    readers: &mut R,
    target: &ManagedSendTarget,
) -> (
    SendFreshEvidence,
    Option<BlindRoot>,
    Option<crate::ffi::AxFrame>,
) {
    let pid = target.pid();
    let wid = target.wid();
    let engine = match readers.engine_facts(target.window()) {
        crate::runtime_core::EngineFactsRead::Managed(facts) => facts,
        crate::runtime_core::EngineFactsRead::Unmanaged => {
            return blind_evidence(BlindRoot::EngineFactsUnmanaged);
        }
        crate::runtime_core::EngineFactsRead::NoReply => {
            return blind_evidence(BlindRoot::EngineFactsNoReply);
        }
    };
    let Some(topo) = readers.fresh_topology().ok() else {
        return blind_evidence(BlindRoot::Topology);
    };
    let live_revision = if topo.revision == engine.revision {
        Some(engine.revision)
    } else {
        None
    };
    let spaces = readers.subject_spaces(wid).ok();
    // Unknown membership collapses: without fresh membership no scope leg
    // is provable.
    if spaces.is_none() {
        return blind_evidence(BlindRoot::Membership);
    }
    let source_space = topo
        .context
        .domains
        .iter()
        .find(|d| d.output == engine.source.output && d.workspace == engine.source.workspace)
        .map(|d| d.space);
    let native_homed = match (&spaces, source_space) {
        (Some(list), Some(space)) => list.as_slice() == [space],
        _ => false,
    };
    let managed_current = Some(engine.source_current && native_homed);
    let ax = readers.subject_ax(pid, wid);
    let fullscreen = match (engine.fullscreen, ax.fullscreen) {
        (true, _) => Some(true),
        (false, Some(state)) => Some(state),
        (false, None) => None,
    };
    let minimized = match (engine.minimized, ax.minimized) {
        (true, _) => Some(true),
        (false, Some(state)) => Some(state),
        (false, None) => None,
    };
    let focused_window = readers
        .selected_focus()
        .and_then(|(focus_pid, focus_wid)| focus_wid.map(|w| format!("{focus_pid}_{w}")));
    // Geometry plus grab selection: the CG entry must own the target on
    // the normal layer with a matching frame; the grab point is the first
    // titlebar candidate where the exact hit element is a plain window AND
    // both hit legs join the exact target. Control/text hits that still
    // join the owner are skipped in order, never grabbed.
    let mut geometry_ok = None;
    let mut grab_point = None;
    let mut ax_hit = None;
    let mut ws_hit = None;
    if let (Some(frame), Some(gctx)) = (ax.frame, readers.grab_context(pid, wid)) {
        let frame_matches = frame.x == gctx.owned.bounds.x
            && frame.y == gctx.owned.bounds.y
            && frame.w == gctx.owned.bounds.w
            && frame.h == gctx.owned.bounds.h;
        if !frame_matches {
            eprintln!(
                "send proof: AX frame {frame:?} mismatches CG bounds {:?} for {pid}_{wid}; refusing without input",
                gctx.owned.bounds,
            );
        }
        geometry_ok = Some(gctx.owned.onscreen && gctx.owned.layer == 0 && frame_matches);
        if let Ok(candidates) = candidate_grab_points(frame) {
            let mut last_detail = String::from("no candidate evaluated");
            for point in candidates {
                let detail = readers.ax_hit_detail(point.0, point.1);
                let ws = readers.ws_hit_wid(point.0, point.1);
                let (ax_owner, role, subrole) = match detail {
                    Some(hit) => (Some(hit.owner), hit.role, hit.subrole),
                    None => (None, None, None),
                };
                if !crate::space_drag_transfer::is_plain_window_hit(
                    role.as_deref(),
                    subrole.as_deref(),
                ) {
                    last_detail = format!(
                        "skipped non-plain AX hit at ({:.0},{:.0}): role {:?} subrole {:?}",
                        point.0, point.1, role, subrole,
                    );
                    continue;
                }
                let ax_param = ax_owner.unwrap_or((0, 0));
                let ws_param = ws.unwrap_or(0);
                match hit_test_input_grab(point, &gctx.owned, &gctx.all, ax_param, ws_param) {
                    Ok(()) => {
                        grab_point = Some(point);
                        ax_hit = ax_owner.map(|(p, w)| format!("{p}_{w}"));
                        ws_hit = ws.map(|w| format!("{pid}_{w}"));
                        break;
                    }
                    Err(detail) => last_detail = detail,
                }
            }
            if grab_point.is_none() {
                eprintln!(
                    "send proof: no input-clear titlebar point for {pid}_{wid} (tried 5; last: {last_detail}); refusing without input",
                );
            }
        }
    }
    let evidence = SendFreshEvidence {
        managed_current,
        fullscreen,
        minimized,
        live_generation: Some(engine.generation),
        live_revision,
        live_source: Some(engine.source.clone()),
        source_current: Some(engine.source_current),
        focused_window,
        focus_agrees: Some(readers.focus_agrees(pid, wid)),
        geometry_ok,
        grab_point,
        ax_hit,
        ws_hit,
    };
    (evidence, None, ax.frame)
}

/// Live managed-send backend over injectable native readers: the production
/// evidence assembly ([`assemble_evidence`]), the single-use permit posts,
/// and the per-post ordinal/revision anchor coherence. Coherence root: one
/// joined [`SendTopology`] per operation (taken in [`snapshot`](
/// ManagedSendBackend::snapshot)); every down-edge re-reads fresh topology
/// and refuses when the target SID no longer sits at its anchored position
/// or the revision moved — a reorder between the row read and the chord
/// post refuses instead of sending to the new occupant. Release edges
/// ([`release_mouse`](ManagedSendBackend::release_mouse) /
/// [`release_key`](ManagedSendBackend::release_key)) bypass the anchor and
/// always attempt. Reused across requests on one owner thread: per-request
/// post state resets in `snapshot` (anchor, goal, held inputs, leg clock);
/// the proven subject is retained so the stay return fence can snapshot
/// and then read the return membership for the same pinned window.
pub struct LiveManagedSendBackend<R: SendNativeReaders> {
    readers: R,
    anchor: Option<TopologyAnchor>,
    subject: Option<(u32, u32)>,
    goal: Option<u64>,
    last_point: Option<(f64, f64)>,
    key_held: Option<(u32, u64)>,
    blind_root: Option<BlindRoot>,
    counts: PostCounts,
    /// Leg clock start (set in `begin_leg`): every native event/readback
    /// line below reports elapsed milliseconds against this base, so the
    /// effective down->drag->chord->up timing compares directly with the
    /// diagnostic `t_ms` series. Logging only; never a gate or timeout.
    leg_t0: Option<std::time::Instant>,
    /// Last proven subject frame plus the backend-selected grab point
    /// (stored in `fresh_evidence` where both are live): event lines
    /// report the grab relative to this frame. Logging only.
    leg_frame: Option<crate::ffi::AxFrame>,
    leg_grab: Option<(f64, f64)>,
    /// Settle-pair change tracking (logging only): the latest membership
    /// read plus the last logged `(membership, current)` pair. Only a
    /// first or changed pair emits a readback line, so the bounded poll
    /// stays quiet while the outcome is stable.
    settle_m: Option<Vec<u64>>,
    settle_prev: Option<(Option<Vec<u64>>, Option<u64>)>,
}

#[derive(Debug, Clone)]
struct TopologyAnchor {
    spaces: Vec<u64>,
    revision: u64,
}

/// Drag-step token for the native event trace (matches the diagnostic
/// `kind` vocabulary: `mouse-move`, `mouse-down`, `mouse-dragged`,
/// `mouse-up`). Pure.
#[must_use]
pub(crate) const fn native_step_name(step: DragStep) -> &'static str {
    match step {
        DragStep::MouseMove => "mouse-move",
        DragStep::MouseDown => "mouse-down",
        DragStep::MouseDragged => "mouse-dragged",
        DragStep::MouseUp => "mouse-up",
        _ => "mouse-unknown",
    }
}

/// One bounded metadata-only native leg-begin line: goal SID plus the
/// backend-selected grab point relative to the proven subject frame
/// (whole points; IDs and geometry only). Pure (the owner emits once per
/// leg in `begin_leg`).
#[must_use]
pub(crate) fn format_native_leg_begin_line(
    goal: u64,
    grab: (f64, f64),
    frame: crate::ffi::AxFrame,
) -> String {
    format!(
        "tiler-macos send native leg-begin goal={goal} grab=({:.0},{:.0}) frame=({:.0},{:.0},{:.0},{:.0}) rel=({:.0},{:.0})",
        grab.0,
        grab.1,
        frame.x,
        frame.y,
        frame.w,
        frame.h,
        grab.0 - frame.x,
        grab.1 - frame.y,
    )
}

/// One bounded metadata-only native mouse-event line at the actual post
/// site: elapsed milliseconds against the leg clock, the posted point, and
/// the point relative to the proven frame when known. Pure.
#[must_use]
pub(crate) fn format_native_mouse_line(
    ev: &str,
    t_ms: u64,
    point: (f64, f64),
    frame: Option<crate::ffi::AxFrame>,
) -> String {
    match frame {
        Some(frame) => format!(
            "tiler-macos send native ev={ev} t_ms={t_ms} pt=({:.0},{:.0}) rel=({:.0},{:.0})",
            point.0,
            point.1,
            point.0 - frame.x,
            point.1 - frame.y,
        ),
        None => format!(
            "tiler-macos send native ev={ev} t_ms={t_ms} pt=({:.0},{:.0})",
            point.0, point.1,
        ),
    }
}

/// One bounded metadata-only native key-event line at the actual post
/// site: elapsed milliseconds, keycode, and flags (hex, like the
/// diagnostic chord notes). Pure.
#[must_use]
pub(crate) fn format_native_key_line(ev: &str, t_ms: u64, keycode: u32, flags: u64) -> String {
    format!("tiler-macos send native ev={ev} t_ms={t_ms} key={keycode:#x} flags={flags:#x}")
}

/// One bounded metadata-only native settle-readback line: elapsed
/// milliseconds, the freshly read membership/current pair, and whether
/// it changed since the last logged pair. The owner emits the first pair
/// (`changed=false` baseline) and each changed pair only, so a stable
/// poll stays quiet. Pure.
#[must_use]
pub(crate) fn format_native_readback_line(
    t_ms: u64,
    membership: &Option<Vec<u64>>,
    current: &Option<u64>,
    changed: bool,
) -> String {
    format!(
        "tiler-macos send native readback t_ms={t_ms} membership={membership:?} current={current:?} changed={changed}"
    )
}

impl<R: SendNativeReaders> LiveManagedSendBackend<R> {
    /// Live backend over one readers seam (production: [`OsSendReaders`];
    /// tests: scripted fakes proving the production assembly path).
    #[must_use]
    pub fn new(readers: R) -> Self {
        Self {
            readers,
            anchor: None,
            subject: None,
            goal: None,
            last_point: None,
            key_held: None,
            blind_root: None,
            counts: PostCounts::default(),
            leg_t0: None,
            leg_frame: None,
            leg_grab: None,
            settle_m: None,
            settle_prev: None,
        }
    }

    /// Elapsed milliseconds against the leg clock (0 before `begin_leg`).
    /// Logging only; never a gate or timeout.
    fn leg_elapsed_ms(&self) -> u64 {
        self.leg_t0
            .map(|t0| t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }

    /// Fresh anchor coherence for one down-edge: the target SID must still
    /// sit at its anchored row position under the anchored revision.
    /// Posts nothing; refuses stale ordinals before any post.
    fn check_anchor(&mut self, goal: u64) -> Result<(), String> {
        let Some(anchor) = self.anchor.clone() else {
            return Err("send leg refused: no topology anchor; refusing".to_owned());
        };
        let topo = self
            .readers
            .fresh_topology()
            .map_err(|e| format!("send leg refused: fresh topology unreadable ({e})"))?;
        if topo.revision != anchor.revision {
            return Err(format!(
                "send leg refused: topology revision moved mid-leg ({} != {}); refusing rather than posting a stale chord",
                topo.revision, anchor.revision
            ));
        }
        let before = anchor.spaces.iter().position(|s| *s == goal);
        let now = topo.row.spaces.iter().position(|s| *s == goal);
        match (before, now) {
            (Some(a), Some(b)) if a == b => Ok(()),
            (Some(_), Some(_)) => Err(
                "send leg refused: target ordinal moved mid-leg; refusing rather than posting a stale chord"
                    .to_owned(),
            ),
            _ => Err("send leg refused: target SID left the row mid-leg; refusing".to_owned()),
        }
    }
}

impl<R: SendNativeReaders> ManagedSendBackend for LiveManagedSendBackend<R> {
    fn read_gates(&mut self) -> SendGates {
        self.readers.read_gates()
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        let topo = self
            .readers
            .fresh_topology()
            .map_err(|e| format!("product send refuses: fresh row unreadable ({e})"))?;
        self.anchor = Some(TopologyAnchor {
            spaces: topo.row.spaces.clone(),
            revision: topo.revision,
        });
        // The proven subject survives this reset on purpose: the stay
        // return fence snapshots after the forward leg, then reads the
        // return membership for the same pinned window. Single-flight
        // serializes operations, and every down-edge re-proves and
        // overwrites the subject before any membership read, so no stale
        // window can leak across operations.
        self.goal = None;
        self.last_point = None;
        self.key_held = None;
        self.leg_t0 = None;
        self.leg_frame = None;
        self.leg_grab = None;
        self.settle_m = None;
        self.settle_prev = None;
        Ok(topo.row)
    }
    fn display_count(&mut self) -> Result<usize, String> {
        self.readers.display_count()
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        self.readers.read_holder(symbolic_id)
    }
    fn fresh_evidence(&mut self, target: &ManagedSendTarget) -> SendFreshEvidence {
        self.subject = Some((target.pid(), target.wid()));
        let (evidence, root, frame) = assemble_evidence(&mut self.readers, target);
        self.blind_root = root;
        // Retain the proven geometry for the native event trace: the
        // adapter pins the first proven grab point, so every later edge
        // logs against this frame. Keep the first good pair per leg
        // (snapshot clears per operation); logging only, never a gate.
        if frame.is_some() {
            self.leg_frame = frame;
        }
        if evidence.grab_point.is_some() {
            self.leg_grab = evidence.grab_point;
        }
        evidence
    }
    fn blind_root(&self) -> Option<BlindRoot> {
        self.blind_root
    }
    fn begin_leg(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String> {
        // Selection only (no OS post): the per-edge proof already ran in
        // the adapter; the anchor re-fences the ordinal here before the
        // grab starts. The permit is consumed (dropped) — one proof never
        // authorizes two legs.
        let _ = permit;
        self.check_anchor(goal)?;
        self.goal = Some(goal);
        // Start the leg clock and the settle-pair log baseline, then emit
        // the single leg-begin trace when the setup proof retained
        // geometry (absent only when no edge proved yet — never a gate).
        self.leg_t0 = Some(std::time::Instant::now());
        self.settle_m = None;
        self.settle_prev = None;
        if let (Some(grab), Some(frame)) = (self.leg_grab, self.leg_frame) {
            eprintln!("{}", format_native_leg_begin_line(goal, grab, frame));
        }
        Ok(())
    }
    fn post_mouse(&mut self, permit: ManagedPostPermit, step: DragStep) -> Result<(), String> {
        let Some(kind) = HidMouseKind::from_drag_step(step) else {
            return Err("refusing non-mouse step on the managed mouse post".to_owned());
        };
        if kind == HidMouseKind::Up {
            let _ = permit;
            return self.release_mouse();
        }
        // Ordinal anchor re-fenced on every grab down-edge: a reorder
        // between the row read and this post refuses instead of dragging
        // under a stale chord.
        let Some(goal) = self.goal else {
            return Err("send leg refused: no anchored goal; refusing".to_owned());
        };
        self.check_anchor(goal)?;
        self.readers
            .hid_mouse(step, permit.point())
            .map_err(|e| format!("managed mouse post refused: {e}"))?;
        if kind == HidMouseKind::Down {
            self.last_point = Some(permit.point());
            self.counts.mouse_down += 1;
        }
        eprintln!(
            "{}",
            format_native_mouse_line(
                native_step_name(step),
                self.leg_elapsed_ms(),
                permit.point(),
                self.leg_frame,
            )
        );
        Ok(())
    }
    fn post_key_down(&mut self, permit: ManagedPostPermit) -> Result<(), String> {
        let Some(goal) = self.goal else {
            return Err("send leg refused: no anchored goal; refusing".to_owned());
        };
        self.check_anchor(goal)?;
        let chord = (permit.keycode(), permit.flags());
        self.readers
            .hid_key(chord.0, chord.1, true)
            .map_err(|e| format!("managed chord post refused: {e}"))?;
        self.key_held = Some(chord);
        self.counts.key_down += 1;
        eprintln!(
            "{}",
            format_native_key_line("chord-down", self.leg_elapsed_ms(), chord.0, chord.1)
        );
        Ok(())
    }
    fn release_mouse(&mut self) -> Result<(), String> {
        // Unconditional release attempt at the last held point; a release
        // with nothing held is a truthful no-op success (no Down ever
        // posted, nothing to release — counts stay balanced).
        let Some(point) = self.last_point.take() else {
            return Ok(());
        };
        self.readers
            .hid_mouse(DragStep::MouseUp, point)
            .map_err(|e| format!("managed mouse release refused: {e}"))?;
        self.counts.mouse_up += 1;
        eprintln!(
            "{}",
            format_native_mouse_line("mouse-up", self.leg_elapsed_ms(), point, self.leg_frame)
        );
        Ok(())
    }
    fn release_key(&mut self) -> Result<(), String> {
        let Some((keycode, flags)) = self.key_held.take() else {
            return Ok(());
        };
        self.readers
            .hid_key(keycode, flags, false)
            .map_err(|e| format!("managed key release refused: {e}"))?;
        self.counts.key_up += 1;
        eprintln!(
            "{}",
            format_native_key_line("key-up", self.leg_elapsed_ms(), keycode, flags)
        );
        Ok(())
    }
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        let (_, wid) = self.subject?;
        let membership = self.readers.subject_spaces(wid).ok();
        // Stage the latest pair for the change-only readback trace below.
        self.settle_m = membership.clone();
        membership
    }
    fn read_current(&mut self) -> Option<u64> {
        let current = self.readers.read_current();
        // Changed-only settle trace: the first pair is the baseline
        // (`changed=false`); later identical polls stay quiet. Logging
        // only — the verdict still reads the returned values.
        let pair = (self.settle_m.clone(), current);
        if self.settle_prev.as_ref() != Some(&pair) {
            let changed = self.settle_prev.is_some();
            eprintln!(
                "{}",
                format_native_readback_line(self.leg_elapsed_ms(), &pair.0, &pair.1, changed)
            );
            self.settle_prev = Some(pair);
        }
        current
    }
    fn counts(&self) -> PostCounts {
        self.counts
    }
    fn select_back(&mut self, source_sid: u64) -> Result<(), String> {
        // Released ordinary return select over the readers seam: the proven
        // SID-accepting goto with exact-current readback. Single-flight,
        // gates, and cancellation are fenced by the stay caller; this only
        // actuates the native select and reports its verdict honestly.
        self.readers.select_space(source_sid)
    }
    fn read_focused(&mut self) -> Option<String> {
        // Honest oracle (the same selected-focus read as the forward
        // proof's focus leg): consulted only when a stay routes, which
        // engine admission refuses while stays are unarmed.
        self.readers
            .selected_focus()
            .and_then(|(pid, wid)| wid.map(|w| format!("{pid}_{w}")))
    }

    fn prepare_trailing_target(
        &mut self,
        pins: &TrailingPrepPins,
    ) -> Result<TrailingPrepProof, String> {
        // Bounded caffeinate companion around preparation only (same shape
        // as the retired main-thread preparation): covers the create plus
        // settle reads and self-terminates via `-t` even if the reaper
        // wedges. Spawn failure refuses before any read.
        let mut awake =
            crate::mc_lifecycle::live::CaffeinateGuard::spawn(TRAILING_PREP_BUDGET_SECS)
                .map_err(|e| format!("trailing send refuses: caffeinate unspawnable ({e})"))?;
        // Single-display rule up front (same rule as the drag below): the
        // pinned legs name exactly one display, never a first-display
        // guess.
        match self.readers.display_count() {
            Ok(1) => {}
            Ok(n) => {
                let _ = awake.stop();
                return Err(format!(
                    "trailing send refuses: expected exactly one native display, found {n}"
                ));
            }
            Err(e) => {
                let _ = awake.stop();
                return Err(format!(
                    "trailing send refuses: display count unreadable ({e})"
                ));
            }
        }
        let mut adapter = OwnerTrailingAdapter {
            readers: &mut self.readers,
            display_index: pins.display_index,
            pinned_cgid: pins.pinned_cgid,
            pinned_uuid: pins.pinned_uuid.clone(),
            budget: ActionBudget::new(TRAILING_PREP_BUDGET_SECS),
            before: None,
            after: None,
        };
        let prep = crate::workspace_send::prepare_trailing_send_target(
            &mut adapter,
            pins.trailing_enabled,
        );
        let _ = awake.stop();
        let prep = prep?;
        let (before, after) = adapter
            .snapshots()
            .ok_or_else(|| "trailing send refuses: post-preparation row unprovable".to_owned())?;
        Ok(TrailingPrepProof {
            target_sid: prep.target_sid,
            created: prep.created,
            revision: prep.revision,
            display_id: before.display.clone(),
            before: before.spaces.clone(),
            after: after.spaces.clone(),
        })
    }
}

/// Preparation budget (seconds) for the owner-side trailing ensure: the
/// same bound as the main-thread select drain it replaces (worst-case
/// single create plus settle reads with margin). No cleanup legs run
/// inside preparation.
const TRAILING_PREP_BUDGET_SECS: u64 = 30;

/// Owner-side trailing preparation adapter: one [`SelectionBackend`] over
/// the send readers seam plus the main-thread-published pinned display
/// legs. Lets the effect owner run the exact same
/// [`prepare_trailing_send_target`](crate::workspace_send::prepare_trailing_send_target)
/// reuse-or-append proof the main thread used to run - same row/
/// emptiness/create guards, same budget shape, same exact-append
/// adoption - with zero AppKit touch: rows come from the readers' fresh
/// SLS topology, the revision is the SLS fingerprint, and the create
/// joins through the pinned `(cgid, managed UUID)` pair verified on every
/// fresh snapshot (never an AppKit read, so this thread neither touches
/// nor blocks the main thread). Records the first and latest snapshots
/// as the engine binding rows (`before`/`after`: identical on reuse).
struct OwnerTrailingAdapter<'r, R: SendNativeReaders> {
    readers: &'r mut R,
    display_index: usize,
    pinned_cgid: u32,
    pinned_uuid: String,
    budget: ActionBudget,
    before: Option<SelectionSnapshot>,
    after: Option<SelectionSnapshot>,
}

impl<'r, R: SendNativeReaders> OwnerTrailingAdapter<'r, R> {
    fn snapshots(&self) -> Option<(&SelectionSnapshot, &SelectionSnapshot)> {
        self.before.as_ref().zip(self.after.as_ref())
    }
}

impl<R: SendNativeReaders> crate::workspace_select::SelectionBackend
    for OwnerTrailingAdapter<'_, R>
{
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        let topo = self
            .readers
            .fresh_topology()
            .map_err(|e| format!("fresh row unreadable ({e})"))?;
        if self.before.is_none() {
            self.before = Some(topo.row.clone());
        }
        self.after = Some(topo.row.clone());
        Ok(topo.row)
    }

    fn display_count(&mut self) -> Result<usize, String> {
        self.readers.display_count()
    }

    fn revision(&mut self) -> Option<u64> {
        self.readers.fresh_topology().ok().map(|topo| topo.revision)
    }

    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        crate::workspace_select::probe_trailing_emptiness(sid)
    }

    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String> {
        if self.snapshot()? != *before {
            return Err("pre-create topology drift".to_owned());
        }
        let raw = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let outcome = crate::mc_lifecycle::live::mc_add_space_pinned(
            self.display_index,
            self.pinned_cgid,
            &self.pinned_uuid,
            &mut self.budget,
            false,
        )
        .map_err(|e| e.to_string())?;
        let after = crate::mc_lifecycle::live::await_snapshot(&|s| {
            crate::mc_probe::validate_create_effect(&raw, s, self.display_index).is_ok()
        })
        .map_err(|e| e.to_string())?;
        crate::mc_probe::validate_create_effect(&raw, &after, self.display_index)
            .map_err(|e| e.to_string())?;
        if let Some(e) = outcome.close_error {
            return Err(format!("created but MC close unproven: {e}"));
        }
        Ok(())
    }

    fn select(&mut self, _before: &SelectionSnapshot, _sid: u64) -> Result<(), String> {
        Err("owner trailing preparation never selects; refusing".to_owned())
    }
}

/// Explicit terminal ownership for one unfinished send-effect owner: a
/// narrowly scoped off-main reaper thread that owns the exact owner until
/// its terminal completion and only then releases the lane. The join blocks
/// only this reaper thread — never the engine, AppKit/main, or any
/// observation owner. The owner itself is bounded (per-edge proof, tagged
/// posts, bounded post-release settle, release-first teardown), so this
/// join terminates; the lane is released exactly once, after the owner can
/// no longer actuate. Spawn failure (resource exhaustion) degrades to the
/// `Drop` backstop with a terminal line: the stop signal was already sent,
/// and the detached owner still self-completes through its release-first
/// executor.
pub(crate) fn spawn_terminal_reaper(thread: ManagedSendThread) {
    if std::thread::Builder::new()
        .name("managed-send-reaper".to_owned())
        .spawn(move || thread.join_and_release())
        .is_err()
    {
        eprintln!(
            "send reaper spawn refused: stop-signalled owner detached (self-completes through release-first teardown)"
        );
    }
}
