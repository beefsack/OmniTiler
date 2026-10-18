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
use crate::runtime::RuntimeHandle;
use crate::runtime_types::NativeContext;
use crate::space_drag_transfer::{
    DragStep, PostCounts, candidate_grab_points, hit_test_input_grab,
};
use crate::synth::HidMouseKind;
use crate::workspace_select::SelectionSnapshot;
use crate::workspace_send::SendGates;
use crate::workspace_send_effect::{ManagedSendTarget, SendFreshEvidence};
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
    /// `None` when the engine no longer manages the window, the query
    /// raced teardown, or the bound elapsed — every one of those refuses
    /// without actuation. This is the managed-window authority: native OS
    /// legs below supplement it but never replace it.
    fn engine_facts(&mut self, window: &str) -> Option<crate::runtime_core::EngineManagedFacts>;
    /// Joined fresh topology (row plus revision plus context from one read
    /// set — the ordinal/revision/homing coherence root).
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
    /// Fresh positional AX hit-test owner at one point (`None` = unknown).
    fn ax_hit_owner(&mut self, x: f64, y: f64) -> Option<(u32, u32)>;
    /// Fresh WindowServer hit-test window at one point (`None` = unknown).
    fn ws_hit_wid(&mut self, x: f64, y: f64) -> Option<u32>;
    /// One tagged HID mouse post (own-switch provenance, zero flags). Only the
    /// transfer executor's mouse vocabulary travels here; chord steps refuse.
    fn hid_mouse(&mut self, step: DragStep, point: (f64, f64)) -> Result<(), String>;
    /// One tagged HID key edge (own-switch provenance).
    fn hid_key(&mut self, keycode: u32, flags: u64, down: bool) -> Result<(), String>;
    /// Fresh current Space for the single display (`None` = unknown).
    fn read_current(&mut self) -> Option<u64>;
}

/// Bound for one engine-managed-facts query from the effect owner: the
/// engine answers from retained truth over the control lane (strict FIFO
/// with observations). A late reply is unknown facts and refuses — never
/// waited out on the owner.
const ENGINE_FACTS_TIMEOUT_MS: u64 = 200;

/// Production native readers over the calling (effect-owner) thread: fresh
/// OS reads plus tagged HID posts. Compiles on every host (off-macOS stubs
/// refuse as unavailable); no new `unsafe` — only the maintained
/// `objc2`/libc wrappers in [`crate::ffi`], [`crate::synth`], and
/// [`crate::focus`].
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
    fn engine_facts(&mut self, window: &str) -> Option<crate::runtime_core::EngineManagedFacts> {
        self.handle.query_managed_facts(
            window,
            std::time::Duration::from_millis(ENGINE_FACTS_TIMEOUT_MS),
        )
    }
    fn fresh_topology(&mut self) -> Result<SendTopology, String> {
        let ns = crate::ffi::ns_display_snapshot()
            .map_err(|e| format!("send topology unreadable: display snapshot ({e})"))?;
        let rows = crate::ffi::sls_display_rows()
            .map_err(|e| format!("send topology unreadable: space rows ({e})"))?;
        let separate = crate::ffi::ns_separate_spaces();
        let snapshot = crate::topology::build_snapshot(&ns, &rows, separate);
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
    fn ax_hit_owner(&mut self, x: f64, y: f64) -> Option<(u32, u32)> {
        crate::ffi::ax_hit_test_owner(x, y).ok()
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

/// Blind evidence: every leg unknown. A single unreadable root (topology,
/// membership, identity) collapses to this — the proof refuses at the
/// first unknown leg without actuation.
fn blind_evidence() -> SendFreshEvidence {
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
    }
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
/// guessed). Grab selection tries the frame-derived titlebar candidates in
/// order and keeps the first point where BOTH the positional AX hit and
/// the WindowServer hit name the pinned target (input-reaching proof, not
/// rectangle stacking); no fully-clear point refuses with no new input
/// (log-only line, zero posts).
fn assemble_evidence<R: SendNativeReaders>(
    readers: &mut R,
    target: &ManagedSendTarget,
) -> SendFreshEvidence {
    let pid = target.pid();
    let wid = target.wid();
    let Some(engine) = readers.engine_facts(target.window()) else {
        return blind_evidence();
    };
    let Some(topo) = readers.fresh_topology().ok() else {
        return blind_evidence();
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
        return blind_evidence();
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
    // titlebar candidate where both hit legs join the exact target.
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
                let ax = readers.ax_hit_owner(point.0, point.1);
                let ws = readers.ws_hit_wid(point.0, point.1);
                let ax_param = ax.unwrap_or((0, 0));
                let ws_param = ws.unwrap_or(0);
                match hit_test_input_grab(point, &gctx.owned, &gctx.all, ax_param, ws_param) {
                    Ok(()) => {
                        grab_point = Some(point);
                        ax_hit = ax.map(|(p, w)| format!("{p}_{w}"));
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
    SendFreshEvidence {
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
    }
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
/// state resets in `snapshot` (the one call [`execute_managed_send`]
/// makes per operation).
pub struct LiveManagedSendBackend<R: SendNativeReaders> {
    readers: R,
    anchor: Option<TopologyAnchor>,
    subject: Option<(u32, u32)>,
    goal: Option<u64>,
    last_point: Option<(f64, f64)>,
    key_held: Option<(u32, u64)>,
    counts: PostCounts,
}

#[derive(Debug, Clone)]
struct TopologyAnchor {
    spaces: Vec<u64>,
    revision: u64,
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
            counts: PostCounts::default(),
        }
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
        self.subject = None;
        self.goal = None;
        self.last_point = None;
        self.key_held = None;
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
        assemble_evidence(&mut self.readers, target)
    }
    fn begin_leg(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String> {
        // Selection only (no OS post): the per-edge proof already ran in
        // the adapter; the anchor re-fences the ordinal here before the
        // grab starts. The permit is consumed (dropped) — one proof never
        // authorizes two legs.
        let _ = permit;
        self.check_anchor(goal)?;
        self.goal = Some(goal);
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
        Ok(())
    }
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        let (_, wid) = self.subject?;
        self.readers.subject_spaces(wid).ok()
    }
    fn read_current(&mut self) -> Option<u64> {
        self.readers.read_current()
    }
    fn counts(&self) -> PostCounts {
        self.counts
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
