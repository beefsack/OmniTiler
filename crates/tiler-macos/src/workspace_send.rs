#![forbid(unsafe_code)]

//! Numbered send-follow controller (armed for 1..9).
//!
//! Product sends 1..9 are armed: the send catalogue rows carry
//! `implemented: true`, the engine admits them, and the product tap table
//! carries their `Ctrl+Shift+digit` chords. The production path is fully
//! wired and proven through scripted-reader tests: engine admission binds the focused managed window
//! plus target SID, revision, lifetime generation, and source domain (see
//! [`crate::runtime_thread::admit_send_claim`] and the send lane), the tile
//! submits admitted claims nonblocking to the dedicated effect owner
//! ([`LiveSendController`]), and the owner re-proves the pinned target per
//! actuation edge over current engine-managed facts plus fresh native reads
//! before posting through the release-first executor.
//!
//! Product route: the 9 send-follow shapes (`Ctrl+Shift+1..9` under the
//! default Ctrl role, see [`crate::workspace_input::is_armed_send`]) move
//! the focused managed window to the target Desktop-N ordinal and follow it
//! there, through the synthetic titlebar-drag plus tagged Desktop-N route:
//! hold a titlebar drag on the subject window, post the target ordinal's
//! enabled standard `Ctrl+digit` chord with the own-switch provenance tag
//! (so the tap passes our own switch chords while still consuming the
//! user's identical chords, see
//! [`crate::input_match::OWN_SWITCH_USER_DATA`]), release, then read back
//! exact membership on the target current. One leg only (never a roundtrip:
//! a failed leg never inverts as a no-op). The release-first executor
//! ([`crate::space_drag_transfer::run_drag_leg`]) guarantees the mouse
//! releases on every path after mouse-down, completes a pending key edge
//! exactly once, and polls the effect only after release inside the bounded
//! [`crate::space_drag_transfer::EFFECT_SETTLE_MS`] window — the only
//! polling on this path. Everything else is single fresh reads (events over
//! polling). No engine or AppKit thread does AX or blocks: the engine
//! admits asynchronously through the claim slot, the tile submits through
//! [`LiveSendController`], and all proof plus settlement runs on the
//! dedicated owner.
//!
//! Subject: the focused managed window pinned at engine admission
//! ([`crate::runtime_types::ClaimedSelect::send_subject`] with its
//! admission-time lifetime generation and source domain) — observed focus,
//! homed to a currently-actuated domain, never fullscreen, never
//! classifier-excluded minimized. Per-edge proof re-reads the current
//! engine-managed facts ([`crate::runtime_core::EngineManagedFacts`]) on
//! the owner and refuses drift — including a pure focus swap, a relaunched
//! lifetime, a removed or excluded subject, or a moved source — instead of
//! grabbing a different window; native membership, focus, frame, and dual
//! hit legs supplement engine authority but never replace it. Target: the
//! ordinal resolves fresh against the joined topology row and must still
//! name the admission-time SID
//! ([`crate::runtime_types::ClaimedSelect::expected_sid`]) under the same
//! engine revision — a reorder between admission and drain refuses instead
//! of sending to the new occupant. Holder: read fresh through the live
//! readers and resolved through
//! [`crate::desktop_shortcuts::resolve_numbered_chord`] — disabled,
//! customized, unreadable, or unmappable holders refuse before any post
//! (never a settings write here). Follow: verified arrival notes the source
//! in the per-display previous history (so `workspace-previous` keeps
//! working) and reports `Applied` under the claim's operation identity so
//! the core settles its retained explicit-`Send` transfer; pre-actuation
//! refusals report `RefusedProven`; ambiguous mid-actuation output
//! (no-effect, unreadable readback, release residue) holds as
//! `PossiblyExecuted` without re-actuation (see
//! [`classify_send_drain_error`]).
//!
//! Ownership: Authentic startup ensures Desktop-N holders once through
//! [`ensure_holders_for_preset`] (enable-only with owned durable preimages,
//! resume-safe, external edits preserved); Compatible performs zero store
//! touch (no read, no write). [`preset_enables_desktop_holders`] names
//! which preset enables. Revert stays explicit
//! ([`crate::desktop_shortcuts::revert_with_store_from`], conditional — a
//! later external edit is preserved, never overwritten); there is no
//! automatic revert on shutdown.
//!
//! Live wiring: the async live-send controller ([`LiveSendController`])
/// plus the production managed-window backend
/// ([`crate::workspace_send_managed::LiveManagedSendBackend`] over the
/// injectable [`crate::workspace_send_managed::SendNativeReaders`] seam)
/// and the dedicated effect owner are implemented and proven through
/// scripted-reader tests. The tile submits engine-admitted claims
/// nonblocking and reconciles completions nonblocking; Authentic startup
/// ensures Desktop-N holders with owned preimages while Compatible stays
/// zero-touch. Only numbered send-follow 1..9 is armed: trailing send-0,
/// stays, relative sends, and output sends stay refused behind the
/// catalogue `implemented` flags and the engine unsupported guard.
/// No modifier-plus-drag product trigger exists.
use std::path::Path;

use tiler_core::directional::OutputId;
use tiler_core::world_report::{AttemptKind, AttemptOutcome};

use crate::desktop_shortcuts::{ApplyOutcome, DesktopShortcutStore, apply_with_store_to};
use crate::input_policy::InputPreset;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction, TileBinding};

/// Session gates for one send effect (plain booleans sampled on the effect
/// owner before any post: plain booleans, no OS touch here). Any denial
/// refuses narrowly without replay and advances nothing — exactly like the
/// select gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendGates {
    pub session_unlocked: bool,
    pub secure_active: bool,
    pub lock_screen: bool,
}

/// Offline send-shape binding subset (armed in product: the 9 numbered
/// send-follow rows `workspace-send-1..9` carry `implemented: true`, so
/// this returns their 9 `Ctrl+Shift+digit` chords).
/// Never adds select chords; trailing send-0, stays, relative sends, and
/// outputs stay out. Driven by the catalogue `implemented` flags plus
/// [`crate::workspace_input::is_armed_send`] (single source with the
/// engine admission gate).
#[must_use]
pub fn bindings() -> Vec<TileBinding> {
    crate::workspace_input::workspace_catalogue()
        .into_iter()
        .filter(|row| row.implemented && crate::workspace_input::is_armed_send(&row.action))
        .filter_map(|row| row.binding())
        .collect()
}

/// Pure ordinal-to-SID resolution over one display's native-order user row:
/// Desktop-N IS the 1-based position (`None` for 0, past-end, or empty
/// rows — never guessed). The drain calls this on its fresh snapshot; the
/// engine admission calls it on the admission-time context row (see
/// [`context_target_sid`]).
#[must_use]
pub fn send_target_sid(spaces_in_order: &[u64], index: u8) -> Option<u64> {
    if index == 0 {
        return None;
    }
    spaces_in_order.get(usize::from(index) - 1).copied()
}

/// Pure admission-time ordinal-to-SID over the native context row for
/// `output`: the display's domains in context order name the native row
/// (see [`crate::runtime_types::NativeContext::derive`]: row order is kept
/// per display for numbered mapping). `None` when the output has no row or
/// the ordinal is out of range — the admission refuses instead of binding a
/// guess.
#[must_use]
pub fn context_target_sid(
    ctx: &crate::runtime_types::NativeContext,
    output: &OutputId,
    index: u8,
) -> Option<u64> {
    if index == 0 {
        return None;
    }
    let mut row: Vec<u64> = Vec::new();
    for domain in &ctx.domains {
        if domain.output != *output {
            continue;
        }
        if !row.contains(&domain.space) {
            row.push(domain.space);
        }
    }
    send_target_sid(&row, index)
}

/// Which preset enables Desktop-N holders. True only under Authentic;
/// Compatible leaves holders to macOS and never touches shortcut
/// settings.
#[must_use]
pub const fn preset_enables_desktop_holders(preset: InputPreset) -> bool {
    matches!(preset, InputPreset::Authentic)
}

/// Authentic Desktop-N holder ensure (called once at tile startup under
/// the Authentic preset; Compatible never reaches here): Desktops
/// 1-9 coverage through the existing explicit-Apply machinery
/// preimage published before the first write, never overwritten (resume
/// replays stored first-observed records), external edits preserved, never
/// overwritten, per-ID failures never block siblings. Returns the terminal
/// [`ApplyOutcome`] (results plus failures plus transfer warnings).
pub fn ensure_authentic_holders<S: DesktopShortcutStore>(
    store: &mut S,
    preimage_path: &Path,
) -> Result<ApplyOutcome, String> {
    apply_with_store_to(
        store,
        crate::desktop_shortcuts::DEFAULT_APPLY_COUNT,
        preimage_path,
    )
}

/// Preset-gated holder ensure for the tile startup path: Authentic ensures
/// through [`ensure_authentic_holders`]; Compatible returns `Ok(None)` with
/// zero store touch (no read, no write — proven by the FakeStore counters).
/// `Err` is an Apply-level refusal (bad count, hostile preimage, lock held,
/// publish failure): sends refuse at the holder leg before any post.
pub fn ensure_holders_for_preset<S: DesktopShortcutStore>(
    preset: InputPreset,
    store: &mut S,
    preimage_path: &Path,
) -> Result<Option<ApplyOutcome>, String> {
    if !preset_enables_desktop_holders(preset) {
        return Ok(None);
    }
    ensure_authentic_holders(store, preimage_path).map(Some)
}

/// Proven outcome of one send drain for attempt feedback (see
/// [`classify_send_drain_error`]): `Applied` settles the retained
/// `Send` transfer on observation confirm, `Refused` settles to
/// observed without actuation (pre-actuation only), `PossiblyExecuted`
/// holds without re-actuation (ambiguous mid-actuation output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendDrainOutcome {
    Applied,
    Refused,
    PossiblyExecuted,
}

/// Wire outcome for one [`SendDrainOutcome`] (pure): transfer kind, never
/// the view kind (selects settle views; sends settle window transfers).
#[must_use]
pub const fn attempt_outcome_for(outcome: SendDrainOutcome) -> AttemptOutcome {
    match outcome {
        SendDrainOutcome::Applied => AttemptOutcome::Applied,
        SendDrainOutcome::Refused => AttemptOutcome::RefusedProven,
        SendDrainOutcome::PossiblyExecuted => AttemptOutcome::PossiblyExecuted,
    }
}

/// Wire kind for a workspace claim action (pure): sends settle transfers,
/// everything else settles views. Single home for the discard/feedback
/// kind branch so a dropped send claim never stages a view row.
#[must_use]
pub fn feedback_kind_for(action: &InputSemanticAction) -> AttemptKind {
    if crate::workspace_input::is_armed_send(action) {
        AttemptKind::Transfer
    } else {
        AttemptKind::View
    }
}

/// Classify one send-drain failure for attempt feedback: prefixes naming
/// pre-actuation refusals (gates, unarmed, display count, subject, target,
/// stale binding, holder, and drag setup/grab/chord-down — the drain never
/// completed a chord there) settle as proven refusals; every other
/// backend/execute failure is ambiguous mid-actuation output and holds as
/// possibly-executed (never blindly repeated). Unknown strings fail safe
/// toward the hold.
#[must_use]
pub fn classify_send_drain_error(reason: &str) -> SendDrainOutcome {
    for prefix in [
        "send gated:",
        "product send refuses",
        "stale bound send discarded:",
        "send drag refused before effect",
    ] {
        if reason.starts_with(prefix) {
            return SendDrainOutcome::Refused;
        }
    }
    SendDrainOutcome::PossiblyExecuted
}

/// Map one dedicated-effect outcome to the drain outcome reported under the
/// claim's operation identity (pure): the effect owner already proved the
/// path, so its verdict transfers directly — `Applied` settles the retained
/// transfer, `RefusedProven` settles to observed, `PossiblyExecuted` holds.
#[must_use]
pub const fn send_drain_outcome_for(
    outcome: crate::workspace_send_effect::SendEffectOutcome,
) -> SendDrainOutcome {
    match outcome {
        crate::workspace_send_effect::SendEffectOutcome::Applied => SendDrainOutcome::Applied,
        crate::workspace_send_effect::SendEffectOutcome::RefusedProven => SendDrainOutcome::Refused,
        crate::workspace_send_effect::SendEffectOutcome::PossiblyExecuted => {
            SendDrainOutcome::PossiblyExecuted
        }
    }
}

/// One settled live send for the main thread to reconcile: the pinned
/// target plus the follow-history attribution (display plus source Space)
/// plus the effect verdict. History advances only on `Applied`; every
/// outcome reports transfer feedback under the claim's operation identity
/// so the core settles its retained transfer — never replayed.
#[derive(Debug, Clone)]
pub struct SendSettle {
    pub target: crate::workspace_send_effect::ManagedSendTarget,
    pub display: String,
    pub from: u64,
    pub outcome: crate::workspace_send_effect::SendEffectOutcome,
}

/// Async live-send controller: the main-thread side of the dedicated
/// native effect. Owns the single [`ManagedSendThread`](
/// crate::workspace_send_managed::ManagedSendThread) across main-loop
/// ticks (spawned lazily on the first submit, so runs that never send
/// never spawn it) and at most one inflight target. Submission and polling
/// are both nonblocking: the effect (engine-facts proof, tagged HID posts,
/// bounded readback) runs on the owner thread, never on the engine or
/// AppKit. Numbered send-follow 1..9 is armed; trailing send-0, stays,
/// relative sends, and outputs stay refused behind the catalogue
/// `implemented` flags and the engine unsupported guard.
///
/// Shutdown policy (explicit terminal): [`shutdown`](
/// LiveSendController::shutdown) takes a terminal nonblocking poll for an
/// already-staged completion first, then requests stop, reaps only a
/// finished owner, and hands an unfinished exact owner to the dedicated
/// terminal reaper ([`spawn_terminal_reaper`](
/// crate::workspace_send_managed::spawn_terminal_reaper)), which joins it
/// off-thread and only then releases the lane. A dropped inflight without
/// a terminal completion settles honestly: `PossiblyExecuted` when the
/// owner may have actuated (lane still busy after the stop request),
/// `RefusedProven` when it certainly did not. Never blocks, never panics.
#[derive(Default)]
pub struct LiveSendController {
    thread: Option<crate::workspace_send_managed::ManagedSendThread>,
    inflight: Option<SendSettleTarget>,
}

#[derive(Debug, Clone)]
struct SendSettleTarget {
    target: crate::workspace_send_effect::ManagedSendTarget,
    display: String,
    from: u64,
}

impl LiveSendController {
    /// Empty controller (no thread, no inflight, no retentions).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a target is inflight (submitted, completion unpolled).
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.inflight.is_some()
    }

    /// Nonblocking submit of one engine-admitted send claim: pins the
    /// target from the claim's admission pins (subject, generation, source,
    /// target SID, revision, operation identity — a claim without admission
    /// pins refuses here instead of actuating), spawns the owner on first
    /// use through `factory` (which runs ON the owner thread, so backend
    /// construction stays off the engine/AppKit), and submits through the
    /// serialized lane. `display`/`from` attribute the later follow-history
    /// note (read fresh by the caller at submit). Refuses busy while a
    /// target is inflight — concurrent presses serialize/refuse instead of
    /// overlapping, and a sent press is never replayed.
    pub fn submit<B, F>(
        &mut self,
        claim: &ClaimedSelect,
        display: &str,
        from: u64,
        factory: F,
    ) -> Result<(), String>
    where
        B: crate::workspace_send_managed::ManagedSendBackend + 'static,
        F: FnOnce() -> B + Send + 'static,
    {
        if self.inflight.is_some() {
            return Err(
                "send-effect-busy: one managed send already inflight; refusing overlap".to_owned(),
            );
        }
        let target = crate::workspace_send_effect::ManagedSendTarget::from_claim(claim)?;
        if self.thread.is_none() {
            self.thread = Some(crate::workspace_send_managed::ManagedSendThread::spawn(
                factory,
            ));
        }
        let Some(thread) = self.thread.as_ref() else {
            return Err("send-effect-busy: no effect owner; refusing".to_owned());
        };
        thread.submit(target.clone()).map_err(|e| e.to_owned())?;
        self.inflight = Some(SendSettleTarget {
            target,
            display: display.to_owned(),
            from,
        });
        Ok(())
    }

    /// Nonblocking poll for the inflight completion: takes it only when it
    /// names the submitted target's operation, lifetime, and revision (a
    /// stale completion drops counted and reports `None` — never replayed,
    /// never applied to the wrong target). Returns the settle for
    /// history/feedback reconciliation, or `None` while the effect runs.
    pub fn poll(&mut self) -> Option<SendSettle> {
        let inflight = self.inflight.as_ref()?;
        let completion = self
            .thread
            .as_ref()?
            .handoff()
            .poll_completion_for(&inflight.target)?;
        let inflight = self.inflight.take()?;
        Some(SendSettle {
            target: inflight.target,
            display: inflight.display,
            from: inflight.from,
            outcome: completion.outcome,
        })
    }

    /// Nonblocking terminal shutdown: a terminal poll for an already-staged
    /// completion first (settled normally when present), then stop request,
    /// finished-only reap, and retention of an unfinished exact owner for
    /// eventual reap. A dropped inflight without a terminal completion
    /// settles honestly (see the struct docs). Returns 0 or 1 settles for
    /// uniform history/feedback reconciliation. Never blocks, never panics.
    pub fn shutdown(&mut self) -> Option<SendSettle> {
        // Terminal nonblocking poll before stopping: an already-staged
        // completion settles exactly like a live poll.
        if let Some(settle) = self.poll() {
            if let Some(thread) = self.thread.take() {
                thread.request_shutdown();
                match thread.shutdown_nonblocking() {
                    // Already stop-signalled above; `shutdown_nonblocking`
                    // re-signals harmlessly. Reaped here.
                    None => {}
                    Some(thread) => {
                        crate::workspace_send_native::spawn_terminal_reaper(thread);
                    }
                }
            }
            return Some(settle);
        }
        let inflight = self.inflight.take();
        if let Some(thread) = self.thread.take() {
            thread.request_shutdown();
            let uncertain = thread.handoff().is_busy();
            match thread.shutdown_nonblocking() {
                None => {}
                Some(thread) => {
                    crate::workspace_send_native::spawn_terminal_reaper(thread);
                }
            }
            return inflight.map(|inflight| SendSettle {
                target: inflight.target,
                display: inflight.display,
                from: inflight.from,
                outcome: if uncertain {
                    crate::workspace_send_effect::SendEffectOutcome::PossiblyExecuted
                } else {
                    crate::workspace_send_effect::SendEffectOutcome::RefusedProven
                },
            });
        }
        None
    }
}

/// One bounded metadata-only readiness line for a scoped tile runtime,
/// emitted once on the main thread at the dispatch-ready boundary: after
/// the runtime started (engine plus input-tap owner ready), the
/// display/space observer installed, and the drag arm resolved (armed or
/// parked with its reason). Input route, preset, scope count, and armed
/// chord count; static config only (no titles, paths, or content). The
/// holder-ensure outcome keeps its own earlier line. Pure (the caller
/// emits); the launch wrapper streams this stderr line live under
/// `--stream`, before any teardown, so a live operator gates dispatch on
/// this observed line instead of sleeping.
#[must_use]
pub fn format_send_readiness_line(
    input: &str,
    preset: &str,
    scopes: usize,
    chords: usize,
) -> String {
    format!(
        "tiler-macos send readiness runtime-started input={input} preset={preset} scopes={scopes} chords={chords}"
    )
}

/// Honest armed chord count for the readiness line: the tap route arms
/// the whole effective catalogue, while the off route arms nothing (the
/// off source parks until stop). Pure (the caller passes the catalogue
/// length it already computed for the terminal outcome).
#[must_use]
pub const fn send_armed_chords(input_is_tap: bool, catalogue_len: usize) -> usize {
    if input_is_tap { catalogue_len } else { 0 }
}

/// One bounded metadata-only admission line for an engine-admitted send
/// claim at submit time: the pinned `"<pid>_<wid>"` subject, the target
/// SID, the source Space the send follows away from, the display, and the
/// operation identity. IDs only (never titles, content, or paths). Pure
/// (the caller emits on the submitted path only); streams live under
/// `--stream` like every stderr line, before any teardown.
#[must_use]
pub fn format_send_admitted_line(
    action: &str,
    subject: &str,
    target: u64,
    from: u64,
    display: &str,
    op: &str,
) -> String {
    format!(
        "tiler-macos send admitted action={action} subject={subject} target={target} from={from} display={display} op={op} submitted complete=false"
    )
}
