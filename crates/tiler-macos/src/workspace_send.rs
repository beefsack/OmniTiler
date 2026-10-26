#![forbid(unsafe_code)]

//! Numbered send controller (armed for 0..9 follow plus 1..9 stay).
//!
//! Product sends 1..9 are armed: the send catalogue rows (follow plus
//! stay) carry
//! `implemented: true`, the engine admits them, and the product tap table
//! carries the follow rows' `Ctrl+Shift+digit` chords (stay rows stay The production path is fully
//! wired and proven through scripted-reader tests: engine admission binds the focused managed window
//! plus target SID, revision, lifetime generation, and source domain (see
//! [`crate::runtime_thread::admit_send_claim`] and the send lane), the tile
//! submits admitted claims nonblocking to the dedicated effect owner
//! ([`LiveSendController`]), and the owner re-proves the pinned target per
//! actuation edge over current engine-managed facts plus fresh native reads
//! before posting through the release-first executor.
//!
//! Product route: the 10 send-follow shapes (`Ctrl+Shift+0..9` under the
//! default Ctrl role, see [`crate::workspace_input::is_armed_send`]) move
//! the focused managed window to the target Desktop-N ordinal and follow it
//! there, bridged-first with drag fallback: the owner runs exactly one
//! direct native bridged move plus the existing Dock-swipe exact-SID
//! selection when the bridged seam is fully available, otherwise the
//! synthetic titlebar-drag plus tagged Desktop-N route below. The drag
//! fallback holds a titlebar drag on the subject window, posts the target
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
//! accepted reply's retained window transfer plus its live native binding
//! names the target SID ([`crate::runtime_types::ClaimedSelect::expected_sid`]
//! carries the reply-bound SID, never an input-ordinal pin); the owner
//! converts that binding to the current live Desktop-N ordinal at dispatch
//! (the native mechanism posts a Desktop-N chord) and refuses when the
//! bound SID left the live row. Holder: read fresh through the live
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
/// zero-touch. Numbered sends 0..9 follow plus 1..9 stay are armed:
/// trailing stay-0, relative sends/stays, and output sends stay refused behind the
/// catalogue `implemented` flags and the engine unsupported guard.
/// No modifier-plus-drag product trigger exists.
use std::path::Path;

use tiler_core::world_report::{FrameSetter, SendSource, WorldCommand};

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

/// Offline send-shape binding subset (armed in product: the 10 numbered
/// send-follow rows `workspace-send-0..9` carry `implemented: true`, so
/// this returns their 10 `Ctrl+Shift+digit` chords; numbered stays 1..9
/// are armed engine-side but bindable unbound, so they contribute no
/// chord here).
/// Never adds select chords; trailing stay-0, relative sends/stays,
/// and
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
/// rows — never guessed). The effect owner converts the reply-bound target
/// SID to its current live ordinal at dispatch (the native mechanism posts
/// a Desktop-N chord); targeting itself never resolves from an input index.
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
/// the ordinal is out of range.
///
/// Offline test seam only: production send targeting is reply-driven (the
/// accepted reply's retained window transfer plus its live native binding
/// authorizes the native send through a live SID lookup), so no production
/// path pins an input-ordinal target pre-reply.
#[cfg(test)]
#[must_use]
pub fn context_target_sid(
    ctx: &crate::runtime_types::NativeContext,
    output: &tiler_core::directional::OutputId,
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

/// Trailing send-follow target decision over one native-order user row
/// (offline pure decision only — no live touch; the caller ensures the
/// Space then binds the managed-send target).
///
/// - `Ok(true)` (last proven empty, even when current): reuse the last SID.
///   Reuse never appends, so the shared bound cannot block it — not even at
///   `MAX_ORDERED`.
/// - `Ok(false)` (last proven occupied): append is required (returns
///   `NeedAppend`; the caller creates exactly once then targets the new
///   SID — never a guess). Refuses at/beyond `MAX_ORDERED` (no append past
///   the shared bound).
/// - `Err` (unknown emptiness, including zero-area/foreign-chrome
///   ambiguity): refuses rather than treating as empty or occupied.
/// - Empty rows or rows containing zero refuse (native row identity
///   unprovable); stay-0 is never constructed through this seam.
///
/// Ordinal note (not decided here): the native Desktop-N posting route
/// only maps ordinals `1..=9` (see `execute_managed_send`: a pinned target
/// past Desktop-9 refuses as unmappable, and preparation refuses before
/// creating an unmappable Space). A trailing send past nine Spaces stays
/// honestly refused — never a relative posting route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailingSendTarget {
    Reuse(u64),
    NeedAppend,
}

pub fn trailing_send_target(
    spaces_in_order: &[u64],
    empty_last: Result<bool, String>,
) -> Result<TrailingSendTarget, String> {
    if spaces_in_order.is_empty() || spaces_in_order.contains(&0) {
        return Err("trailing send: native row identity unprovable".to_owned());
    }
    match empty_last {
        Ok(true) => Ok(TrailingSendTarget::Reuse(
            *spaces_in_order.last().expect("nonempty row"),
        )),
        Ok(false) => {
            if spaces_in_order.len() >= tiler_core::workspace::MAX_ORDERED {
                return Err("shared workspace catalogue bound reached".to_owned());
            }
            Ok(TrailingSendTarget::NeedAppend)
        }
        Err(reason) => Err(format!(
            "trailing send refuses: emptiness unknown ({reason})"
        )),
    }
}

/// Drain route for one engine-admitted send claim (pure, no native
/// touch): bound targets keep the pinned numbered lane; an unbound
/// follow target whose current correlated reply carries a matched
/// explicit creation desire (all three desire legs present and naming
/// the retained target) rides the desire-driven creation executor;
/// everything else stays pinned (unbound refusals settle downstream
/// without actuation — missing bindings never create). The public
/// index rides unchanged for diagnostics (never forced to 0, never an
/// invented desire/binding). Malformed indexes (>9), stays, bound
/// targets, and missing/mismatched desires all stay pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendDrainRoute {
    Pinned,
    CreationBacked,
}

/// Pure drain-route decision shared by the production tile drain and
/// its regression tests: production submits through the returned lane
/// and tests execute the same branch with a scripted backend.
#[must_use]
pub fn route_send_claim_for_drain(claim: &ClaimedSelect) -> SendDrainRoute {
    match &claim.action {
        crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index,
            follow: true,
        } if *index <= 9 => {}
        _ => return SendDrainRoute::Pinned,
    }
    if claim.expected_sid.is_some() {
        return SendDrainRoute::Pinned;
    }
    match (
        claim.select_creation_operation.as_deref(),
        claim.select_creation_logical.as_deref(),
        claim.select_creation_output.as_deref(),
        claim.select_target_workspace.as_deref(),
        claim.select_target_output.as_deref(),
    ) {
        (Some(_), Some(logical), Some(output), Some(target_workspace), Some(target_output))
            if logical == target_workspace && output == target_output =>
        {
            SendDrainRoute::CreationBacked
        }
        _ => SendDrainRoute::Pinned,
    }
}

/// Main-thread-published display legs for one creation-backed send
/// submit (pure, plain data only): exactly one published display with
/// a pinned non-empty UUID and a nonzero CG ID. Anything else refuses
/// before any submit — never a first-display guess, never a guessed
/// identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendCreationLegs {
    pub display_index: usize,
    pub pinned_cgid: u32,
    pub pinned_uuid: String,
}

/// Pure display-legs resolution shared by the production tile drain
/// and its regression tests: production submits through the resolved
/// legs and tests execute the same branch with scripted inputs.
pub fn send_creation_legs_for_drain(
    published: Option<&crate::runtime_shared::PublishedDisplayInputs>,
) -> Result<SendCreationLegs, String> {
    let Some(published) = published else {
        return Err("product send refuses: published display legs unpublished".to_owned());
    };
    if published.displays.len() != 1 {
        return Err("product send refuses: published display legs unpublished".to_owned());
    }
    let display = &published.displays[0];
    match (display.uuid.clone(), display.cg_id) {
        (Some(uuid), cgid) if !uuid.is_empty() && cgid != 0 => Ok(SendCreationLegs {
            display_index: 0,
            pinned_cgid: cgid,
            pinned_uuid: uuid,
        }),
        _ => Err("product send refuses: published display legs unpinned".to_owned()),
    }
}

/// Prepared trailing send-follow target: the exact native SID the owner must
/// drag to, the runtime-created SID when an append ran (`None` on reuse),
/// and the fresh post-preparation topology revision the owner fences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrailingSendPrep {
    pub target_sid: u64,
    pub created: Option<u64>,
    pub revision: u64,
}

/// Typed trailing preparation failure: the refusing reason plus the
/// mechanical native disposition at the actual touch and the genuine rows
/// around it. `Refused` means nothing was actuated (pre-actuation gates,
/// fences, drift — the backend's typed refusal, never a string guess);
/// `Threw` means native work may have run (partial or erroneous dispatch,
/// unreadable/invalid post states — never blindly repeated). `before` is
/// the genuine pre row when one was read (`None` when the pre row itself
/// was unreadable — nothing to defer); `after` is the genuine post row when
/// one was read and validated (`None` where unreadable/invalid — unknown
/// holds, never a fabricated post-order). `dispatched` is true only when
/// the backend create call itself reported success (the post paths); a
/// refused or partial create never sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingPrepFailure {
    pub reason: String,
    pub setter: tiler_core::world_report::FrameSetter,
    pub before: Option<Box<crate::workspace_select::SelectionSnapshot>>,
    pub after: Option<Box<crate::workspace_select::SelectionSnapshot>>,
    pub dispatched: bool,
}

impl TrailingPrepFailure {
    fn refused(reason: String, before: Option<crate::workspace_select::SelectionSnapshot>) -> Self {
        Self {
            reason,
            setter: tiler_core::world_report::FrameSetter::Refused,
            before: before.map(Box::new),
            after: None,
            dispatched: false,
        }
    }

    fn threw(
        reason: String,
        before: Option<crate::workspace_select::SelectionSnapshot>,
        after: Option<crate::workspace_select::SelectionSnapshot>,
    ) -> Self {
        Self {
            reason,
            setter: tiler_core::world_report::FrameSetter::Threw,
            before: before.map(Box::new),
            after: after.map(Box::new),
            dispatched: false,
        }
    }
}

/// Ensure the trailing send-follow target over a selection backend (the same
/// [`crate::workspace_select::SelectionBackend`] trait the select drain
/// proves through, so scripted fakes inject here and the live tile passes
/// its native backend): reuse the proven-empty last, else exactly one
/// append — or exactly one append unconditionally when `require_append`
/// carries an explicit backing desire. No live effects beyond the single
/// append; unknown emptiness refuses without touching Mission Control
/// (on the reuse-or-append path only).
///
/// An explicit desire requires exactly one append even when the last
/// proves empty: the core already resolved occupancy and requests
/// backing for a different unbound logical target, so reusing the
/// emptied native would retarget the wrong logical and stage no
/// creation receipt. Occupancy is not consulted on that path (an
/// unreadable emptiness read neither blocks nor guesses); the fresh
/// row, order, ordinal-bound, display, revision, and exact-append
/// adoption gates still apply unchanged, as do the truthful ambiguous
/// receipts.
///
/// Guards, in order: capability (unarmed refuses before any read), fresh
/// row identity, emptiness proof (unknown refuses — zero-area and
/// foreign-chrome candidates read unknown through the backend, never
/// empty), Desktop-N mappability (reuse past position 9 or an append that
/// would land past 9 refuses BEFORE creating an unmappable Space — the
/// owner would refuse the same ordinal identically, so preparation never
/// strands a creation), then one backend append with exact-append adoption
/// (same-display, same current, length + 1, strict prefix — any drift
/// refuses instead of adopting a foreign SID).
///
/// Revision note: the returned revision is read fresh AFTER preparation
/// (post-append on the create path), so an own-append revision move is
/// carried explicitly into [`bind_trailing_send_target`] — never bypassed.
/// The owner still fences that revision plus focus/generation/source per
/// edge, so any later drift refuses.
///
/// The typed failure preserves the backend's [`DispatchError`] setter
/// verbatim (never a string classifier): a refused create stays `Refused`
/// even when its reason names no proven prefix, and a partial create stays
/// `Threw`. The caller retains a deferred creation receipt from the genuine
/// rows exactly like the select drain (emit nothing fabricated); the reason
/// strings stay stable so [`classify_trailing_prep_error`] keeps mapping
/// the outcome.
pub fn prepare_trailing_send_target(
    backend: &mut impl crate::workspace_select::SelectionBackend,
    trailing_enabled: bool,
    require_append: bool,
) -> Result<TrailingSendPrep, TrailingPrepFailure> {
    if !trailing_enabled {
        return Err(TrailingPrepFailure::refused(
            "trailing send-0 unarmed: no trailing append runs".to_owned(),
            None,
        ));
    }
    let before = match backend.snapshot() {
        Ok(row) => row,
        Err(e) => {
            return Err(TrailingPrepFailure::refused(
                format!("trailing send refuses: fresh row unreadable ({e})"),
                None,
            ));
        }
    };
    if let Err(e) = before.validate() {
        return Err(TrailingPrepFailure::refused(
            format!("trailing send refuses: fresh row unprovable ({e})"),
            None,
        ));
    }
    let last = *before.spaces.last().expect("validated nonempty row");
    // An explicit backing desire forces the append decision without
    // consulting occupancy: `Ok(false)` can only resolve to `NeedAppend`
    // or a shared-bound refusal here (never a reuse, never a guess), so
    // the row/order/ordinal gates below run unchanged.
    let decision = if require_append {
        match trailing_send_target(&before.spaces, Ok(false)) {
            Ok(TrailingSendTarget::NeedAppend) => Ok(TrailingSendTarget::NeedAppend),
            Ok(TrailingSendTarget::Reuse(_)) => Err(
                "trailing send refuses: explicit backing requires an append; refusing".to_owned(),
            ),
            Err(reason) => Err(reason),
        }
    } else {
        trailing_send_target(&before.spaces, backend.empty(last))
    };
    match decision {
        Err(reason) => Err(TrailingPrepFailure::refused(reason, Some(before))),
        Ok(TrailingSendTarget::Reuse(sid)) => {
            if before.spaces.len() > 9 {
                return Err(TrailingPrepFailure::refused(
                    "trailing send refuses: reuse ordinal past Desktop-9; unmappable".to_owned(),
                    Some(before),
                ));
            }
            let revision = match backend.revision() {
                Some(revision) => revision,
                None => {
                    // No create ran on the reuse path, so there is no
                    // create pre-order to defer: `before` stays `None`.
                    return Err(TrailingPrepFailure::threw(
                        "trailing send refuses: post-preparation revision unprovable".to_owned(),
                        None,
                        None,
                    ));
                }
            };
            Ok(TrailingSendPrep {
                target_sid: sid,
                created: None,
                revision,
            })
        }
        Ok(TrailingSendTarget::NeedAppend) => {
            if before.spaces.len() + 1 > 9 {
                return Err(TrailingPrepFailure::refused(
                    "trailing send refuses: append ordinal past Desktop-9; unmappable".to_owned(),
                    Some(before),
                ));
            }
            if let Err(e) = backend.create(&before) {
                // Typed disposition, never a string classifier: `Refused`
                // means no native effect was attempted; `Threw` means a
                // partial or erroneous dispatch may have run. The genuine
                // post row (when readable) rides along so the caller can
                // prove refusal or defer the receipt — never a guess.
                let after = backend
                    .snapshot()
                    .ok()
                    .filter(|row| row.display == before.display && row.validate().is_ok());
                return Err(TrailingPrepFailure {
                    reason: format!("trailing send refuses: append failed ({e})"),
                    setter: e.setter,
                    before: Some(Box::new(before)),
                    after: after.map(Box::new),
                    dispatched: false,
                });
            }
            let after = match backend.snapshot() {
                Ok(row) => row,
                Err(e) => {
                    return Err(TrailingPrepFailure::threw(
                        format!("trailing send refuses: post-append row unreadable ({e})"),
                        Some(before),
                        None,
                    ));
                }
            };
            if let Err(e) = after.validate() {
                return Err(TrailingPrepFailure::threw(
                    format!("trailing send refuses: post-append row unprovable ({e})"),
                    Some(before),
                    None,
                ));
            }
            if after.display != before.display
                || after.current != before.current
                || after.spaces.len() != before.spaces.len() + 1
                || !after.spaces.starts_with(&before.spaces)
            {
                // Dispatched but not an exact same-display append. The
                // genuine rows ride along: a readable no-delta row defers
                // (async arrival may still land) while a disagreeing row
                // proves ambiguity with no guessed SID.
                return Err(TrailingPrepFailure {
                    reason: "trailing send refuses: create effect is not an exact same-display append; adoption refused"
                        .to_owned(),
                    setter: tiler_core::world_report::FrameSetter::Accepted,
                    before: Some(Box::new(before)),
                    after: Some(Box::new(after)),
                    dispatched: true,
                });
            }
            let sid = *after.spaces.last().expect("appended row");
            let revision = match backend.revision() {
                Some(revision) => revision,
                None => {
                    return Err(TrailingPrepFailure::threw(
                        "trailing send refuses: post-append revision unprovable".to_owned(),
                        Some(before),
                        Some(after),
                    ));
                }
            };
            Ok(TrailingSendPrep {
                target_sid: sid,
                created: Some(sid),
                revision,
            })
        }
    }
}

/// Bind a prepared trailing target to the pinned managed-send identity from
/// an engine-admitted trailing claim.
///
/// Pin contract: the claim must carry the admission pins for the ORIGINAL
/// window — subject `"<pid>_<wid>"`, app-lifetime generation, source
/// domain, and operation identity — naming it exactly. A replacement focus
/// window, a relaunched lifetime, a moved source, a non-trailing action, or
/// a stay-0 refuses instead of substituting or guessing.
///
/// Revision contract (explicit, no fence bypass): the engine cannot pin a
/// trailing SID at admission (the Space may not exist yet), so the claim's
/// `expected_sid`/`topology_revision` are NOT trusted here — the fresh
/// post-preparation `prep` supplies both the exact ensured SID and the
/// post-append revision. When preparation appended, the target MUST be the
/// exact-created SID (never a drifted occupant). The dedicated owner then
/// fences that revision plus focus/generation/source per actuation edge
/// ([`prove_managed_send`](crate::workspace_send_effect::prove_managed_send)),
/// so the own-append revision move is carried explicitly and any later
/// drift refuses without actuation.
pub fn bind_trailing_send_target(
    claim: &ClaimedSelect,
    prep: TrailingSendPrep,
) -> Result<crate::workspace_send_effect::ManagedSendTarget, String> {
    match &claim.action {
        crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        } => {}
        _ => {
            return Err(
                "send target unpinned: trailing bind needs send-0 follow; refusing".to_owned(),
            );
        }
    }
    if prep.target_sid == 0 {
        return Err("send target unpinned: zero trailing SID; refusing".to_owned());
    }
    if let Some(created) = prep.created {
        if created == 0 {
            return Err("send target unpinned: zero created SID; refusing".to_owned());
        }
        if created != prep.target_sid {
            return Err(
                "send target unpinned: prepared target is not the exact-created SID; refusing"
                    .to_owned(),
            );
        }
    }
    let subject = claim.send_subject.as_deref().ok_or_else(|| {
        "send target unpinned: claim carries no subject window; refusing".to_owned()
    })?;
    let (pid, wid) = crate::runtime_types::parse_window_id(subject)
        .ok_or_else(|| format!("send target unpinned: subject {subject:?} unparsable; refusing"))?;
    let app_generation = claim.send_generation.ok_or_else(|| {
        "send target unpinned: claim carries no app generation; refusing".to_owned()
    })?;
    let source = claim.send_source.clone().ok_or_else(|| {
        "send target unpinned: claim carries no source domain; refusing".to_owned()
    })?;
    let operation_id = claim.operation_id.as_deref().ok_or_else(|| {
        "send target unpinned: claim carries no operation identity; refusing".to_owned()
    })?;
    let target = crate::workspace_send_effect::ManagedSendTarget::bind(
        pid,
        wid,
        app_generation,
        source,
        prep.target_sid,
        prep.revision,
        operation_id,
    )?;
    if target.window() != subject {
        return Err(format!(
            "send target unpinned: subject {subject:?} mismatches pinned PID/WID; refusing"
        ));
    }
    Ok(target)
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

/// Setter disposition for one [`SendDrainOutcome`] (pure): only a proven
/// dispatch stages `Accepted` (arrival is never claimed here); proven
/// pre-actuation refusals stage `Refused`; ambiguous mid-actuation output
/// stages `Threw` (never blindly repeated). The core derives every verdict
/// from the raw row plus the staged observation.
#[must_use]
pub const fn setter_for(outcome: SendDrainOutcome) -> FrameSetter {
    match outcome {
        SendDrainOutcome::Applied => FrameSetter::Accepted,
        SendDrainOutcome::Refused => FrameSetter::Refused,
        SendDrainOutcome::PossiblyExecuted => FrameSetter::Threw,
    }
}

/// Whether a workspace claim action settles a window transfer (pure):
/// sends settle transfers, everything else settles views. Single home for
/// the discard/feedback lane branch so a dropped send claim never stages
/// a view row. Numbered sends settle transfers regardless of follow intent
/// (stays transfer the mover and return the view - never a view switch).
#[must_use]
pub fn is_send_action(action: &InputSemanticAction) -> bool {
    matches!(action, InputSemanticAction::WorkspaceSend { .. })
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

/// Classify one owner-side trailing preparation failure: reasons naming
/// pre-actuation refusals (unarmed capability, unreadable/unprovable fresh
/// rows, unknown emptiness, Desktop-9 unmappability decided before any
/// create, and pre-create drift - the owner never touched the desktop)
/// refuse proven; every other preparation failure (press failures,
/// post-append unreadable rows, inexact appends, unprovable revisions,
/// unproven closes) is ambiguous mid-actuation output and holds as
/// possibly-executed, never replayed. Unknown strings fail safe toward the
/// hold - same rule as [`classify_send_drain_error`].
#[must_use]
pub fn classify_trailing_prep_error(
    reason: &str,
) -> crate::workspace_send_effect::SendEffectOutcome {
    use crate::workspace_send_effect::SendEffectOutcome;
    for prefix in [
        "trailing send-0 unarmed: no trailing append runs",
        "trailing send refuses: caffeinate unspawnable",
        "trailing send refuses: expected exactly one native display",
        "trailing send refuses: display count unreadable",
        "trailing send refuses: fresh row unreadable",
        "trailing send refuses: fresh row unprovable",
        "trailing send refuses: emptiness unknown",
        "trailing send refuses: reuse ordinal past Desktop-9; unmappable",
        "trailing send refuses: append ordinal past Desktop-9; unmappable",
        "trailing send refuses: append failed (pre-create topology drift)",
        "trailing preparation unimplemented on this backend; refusing",
    ] {
        if reason.starts_with(prefix) {
            return SendEffectOutcome::RefusedProven;
        }
    }
    SendEffectOutcome::PossiblyExecuted
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
/// plus the effect verdict, the owner's raw setter disposition, and the
/// reply-published follow view identity. History advances only on
/// `Applied` follows; every outcome reports transfer feedback under the
/// claim's operation identity so the core settles its retained transfer -
/// never replayed.
#[derive(Debug, Clone)]
pub struct SendSettle {
    pub target: crate::workspace_send_effect::ManagedSendTarget,
    pub display: String,
    pub from: u64,
    pub follow: bool,
    pub outcome: crate::workspace_send_effect::SendEffectOutcome,
    /// Owner's raw native setter disposition captured at the actual write
    /// boundary. The drain stages it verbatim; it is never derived from
    /// the arrival outcome.
    pub setter: tiler_core::world_report::FrameSetter,
    /// Reply-published follow view flight identity for `follow=true`
    /// sends (read from the reply's own transfers at dispatch). The drain
    /// stages the follow view attempt under exactly this identity; `None`
    /// stages no view and invents none.
    pub follow_view_operation: Option<String>,
    /// Owner-verified exact-append proof for trailing executions that
    /// appended (`None` for numbered sends and trailing reuse). `created`
    /// rides with or without a token so the drain adopts owned creation;
    /// only `binding` is token-gated. The drain forwards the binding to
    /// the engine store verbatim - never re-derived, never guessed.
    pub trailing: Option<crate::workspace_send_effect::TrailingCompletionProof>,
    /// Genuine owner reads around the dispatch (see
    /// [`crate::workspace_send_effect::SendCompletion`]): the pre-write
    /// homing/current pair plus the verified-or-reread post pair. The
    /// main thread stages them as raw attempt evidence; `None` legs are
    /// unreadable, never guessed.
    pub pre_member: Option<u64>,
    pub post_member: Option<u64>,
    pub pre_view: Option<u64>,
    pub post_view: Option<u64>,
    /// Observation-time native current for the target output captured at
    /// dispatch. Stages as the follow view attempt's `native_baseline`;
    /// `None` when unread at dispatch.
    pub baseline: Option<u64>,
}

/// Whether one settled send advances per-display previous history (pure):
/// only verified follow arrivals do. Stay settlements keep the source
/// current, so they never note an arrival even when applied.
#[must_use]
pub const fn should_note_history(outcome: SendDrainOutcome, follow: bool) -> bool {
    matches!(outcome, SendDrainOutcome::Applied) && follow
}

/// Async live-send controller: the main-thread side of the dedicated
/// native effect. Owns the single [`ManagedSendThread`](
/// crate::workspace_send_managed::ManagedSendThread) across main-loop
/// ticks (spawned lazily on the first submit, so runs that never send
/// never spawn it) and at most one inflight target. Submission and polling
/// are both nonblocking: the effect (engine-facts proof, tagged HID posts,
/// bounded readback) runs on the owner thread, never on the engine or
/// AppKit. Numbered send-follow 0..9 plus stay 1..9 is armed; trailing
/// stay-0, relative sends, and outputs stay refused behind the catalogue
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
    inflight: Option<SendInflight>,
}

#[derive(Debug, Clone)]
enum SendInflight {
    Pinned(SendSettleTarget),
    Trailing(TrailingInflight),
}

#[derive(Debug, Clone)]
struct SendSettleTarget {
    target: crate::workspace_send_effect::ManagedSendTarget,
    display: String,
    from: u64,
    /// Observation-time native current for the target output captured at
    /// dispatch (the reply plan baseline). Rides the settle so the main
    /// thread stages the follow view attempt truthfully.
    baseline: Option<u64>,
    /// Reply-published follow view flight identity, copied from the bound
    /// claim plan at submit. Rides the settle so the drain stages the
    /// follow view attempt under the reply's own identity.
    follow_view_operation: Option<String>,
}

#[derive(Debug, Clone)]
struct TrailingInflight {
    request: crate::workspace_send_effect::TrailingSendRequest,
    display: String,
    from: u64,
}

/// Main-thread-published legs for one trailing owner submit: the
/// plain-data display identity (never an AppKit read here), the
/// follow-history attribution, and the capability flag. Bundled so the
/// submit stays within the argument-count lint.
pub struct TrailingSubmit<'a> {
    pub display_index: usize,
    pub pinned_cgid: u32,
    pub pinned_uuid: &'a str,
    pub display: &'a str,
    pub from: u64,
    pub trailing_enabled: bool,
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
    /// target from the claim's reply-bound plan (subject, generation,
    /// source, reply-bound target SID, revision, operation identity, follow
    /// intent plus the stay-only source SID - a claim without bound pins
    /// refuses here instead of actuating), spawns the owner on first use through
    /// `factory` (which runs ON the owner thread, so backend construction
    /// stays off the engine/AppKit), and submits through the serialized
    /// lane. `display`/`from` attribute the later follow-history note (read
    /// fresh by the caller at submit; stays never note history). Refuses
    /// busy while a target is inflight - concurrent presses
    /// serialize/refuse instead of overlapping, and a sent press is never
    /// replayed. The owner routes on the pinned intent (forward bridged move
    /// plus destination select with drag fallback for follows, plus the
    /// released return select for stays).
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
        self.inflight = Some(SendInflight::Pinned(SendSettleTarget {
            target,
            display: display.to_owned(),
            from,
            baseline: claim.select_baseline_sid,
            follow_view_operation: claim.send_follow_view_operation.clone(),
        }));
        Ok(())
    }

    /// Nonblocking submit of one engine-admitted trailing send-0 claim:
    /// builds the owner request from the claim's original pins (subject,
    /// lifetime generation, source domain, operation identity, pending
    /// token) plus the caller-published plain-data display legs - never an
    /// AppKit read here - spawns the owner on first use through `factory`
    /// (which runs ON the owner thread), and submits through the
    /// serialized lane. The owner prepares the exact native target itself
    /// (reuse-or-append), binds it, and actuates the single drag; the
    /// main thread only polls the completion. `display`/`from` attribute
    /// the later follow-history note. Refuses busy while a target is
    /// inflight, and refuses unpinned claims/displays before any submit -
    /// a sent press is never replayed.
    pub fn submit_trailing<B, F>(
        &mut self,
        claim: &ClaimedSelect,
        legs: TrailingSubmit<'_>,
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
        let request = crate::workspace_send_effect::TrailingSendRequest::from_claim(
            claim,
            legs.display_index,
            legs.pinned_cgid,
            legs.pinned_uuid,
            legs.trailing_enabled,
        )?;
        if self.thread.is_none() {
            self.thread = Some(crate::workspace_send_managed::ManagedSendThread::spawn(
                factory,
            ));
        }
        let Some(thread) = self.thread.as_ref() else {
            return Err("send-effect-busy: no effect owner; refusing".to_owned());
        };
        thread
            .submit_trailing(request.clone())
            .map_err(|e| e.to_owned())?;
        self.inflight = Some(SendInflight::Trailing(TrailingInflight {
            request,
            display: legs.display.to_owned(),
            from: legs.from,
        }));
        Ok(())
    }

    /// Nonblocking poll for the inflight completion: takes it only when it
    /// names the submitted target's operation, lifetime, revision, and
    /// intent (a stale completion drops counted and reports `None` -
    /// never replayed, never applied to the wrong target). Trailing
    /// requests match on operation plus lifetime (the revision is
    /// owner-prepared and rides back in the completion) and rebuild the
    /// bound target from the completion proof. Returns the
    /// settle for history/feedback reconciliation, or `None` while the
    /// effect runs. The settle carries the pinned follow intent so the
    /// drain notes previous history on applied follows only, plus the
    /// optional trailing proof for the engine binding store.
    pub fn poll(&mut self) -> Option<SendSettle> {
        let inflight = self.inflight.as_ref()?;
        let handoff = self.thread.as_ref()?.handoff();
        match inflight {
            SendInflight::Pinned(pinned) => {
                let completion = handoff.poll_completion_for(&pinned.target)?;
                let inflight = self.inflight.take()?;
                let SendInflight::Pinned(pinned) = inflight else {
                    return None;
                };
                let follow = pinned.target.follow();
                Some(SendSettle {
                    target: pinned.target,
                    display: pinned.display,
                    from: pinned.from,
                    follow,
                    outcome: completion.outcome,
                    setter: completion.setter,
                    follow_view_operation: pinned.follow_view_operation,
                    trailing: None,
                    pre_member: completion.pre_member,
                    post_member: completion.post_member,
                    pre_view: completion.pre_view,
                    post_view: completion.post_view,
                    baseline: pinned.baseline,
                })
            }
            SendInflight::Trailing(trailing) => {
                let completion = handoff.poll_trailing_completion_for(&trailing.request)?;
                let inflight = self.inflight.take()?;
                let SendInflight::Trailing(trailing) = inflight else {
                    return None;
                };
                // Rebuild the owner-bound target from the completion proof
                // (proven present: the handoff only stages proof-bearing
                // completions on this path - same expect style as the
                // select drain's proven invariants).
                let proof = completion
                    .trailing
                    .clone()
                    .expect("proof path carries proof");
                let target = crate::workspace_send_effect::ManagedSendTarget::bind(
                    trailing.request.pid,
                    trailing.request.wid,
                    trailing.request.app_generation,
                    trailing.request.source.clone(),
                    proof.target_sid,
                    proof.revision,
                    &trailing.request.operation_id,
                )
                .ok()?;
                if target.window() != trailing.request.window {
                    return None;
                }
                Some(SendSettle {
                    target,
                    display: trailing.display,
                    from: trailing.from,
                    follow: true,
                    outcome: completion.outcome,
                    setter: completion.setter,
                    // The trailing owner path carries no reply-published
                    // view identity to the main thread (unbound trailing
                    // binds none): no view is staged and none is invented.
                    follow_view_operation: None,
                    trailing: completion.trailing.clone(),
                    pre_member: completion.pre_member,
                    post_member: completion.post_member,
                    pre_view: completion.pre_view,
                    post_view: completion.post_view,
                    baseline: None,
                })
            }
        }
    }

    /// Nonblocking poll for a trailing request's proof-less completion
    /// (preparation refused or was cancelled before binding): feedback-only
    /// settlement under the request's operation identity - no target to
    /// rebuild, no history to note, never replayed. Returns `None` while
    /// the effect runs or when the inflight request is a pinned send.
    pub fn poll_trailing_refusal(&mut self) -> Option<TrailingRefusal> {
        let SendInflight::Trailing(trailing) = self.inflight.as_ref()? else {
            return None;
        };
        let completion = self
            .thread
            .as_ref()?
            .handoff()
            .poll_trailing_refusal_for(&trailing.request)?;
        let inflight = self.inflight.take()?;
        let SendInflight::Trailing(trailing) = inflight else {
            return None;
        };
        debug_assert_eq!(completion.operation_id, trailing.request.operation_id);
        Some(TrailingRefusal {
            operation_id: trailing.request.operation_id,
            setter: completion.setter,
        })
    }

    /// Observe retained trailing creation receipts against one fresh native
    /// row: the first successfully observed full post-order completes that
    /// desire exactly once (original dispatch stamp preserved) with no
    /// claimed identity; unreadable or mismatched-display rows stay retained
    /// with nothing submitted. Same seam as the select lane (see
    /// [`crate::select_owner::SelectController::observe_creation_receipts`]):
    /// trailing appends whose post-order was not yet readable at preparation
    /// settle here instead of fabricating a post-order on the owner thread.
    pub fn observe_trailing_creation_receipts(
        &self,
        row: &crate::workspace_select::SelectionSnapshot,
        read_started_ms: u64,
    ) -> Vec<crate::workspace_select::CreationReport> {
        let Some(thread) = self.thread.as_ref() else {
            return Vec::new();
        };
        let handoff = thread.handoff();
        let mut pending = handoff.take_pending_creations();
        let mut ready = Vec::new();
        pending.retain(|receipt| {
            if let Some(report) = receipt.observe(row, read_started_ms) {
                ready.push(report);
                false
            } else {
                true
            }
        });
        for receipt in pending {
            handoff.retain_pending_creation(receipt);
        }
        ready
    }

    /// Stop the owner thread without joining: signal stop, reap only when
    /// already finished, and hand an unfinished exact owner to the
    /// dedicated terminal reaper (which joins it off-thread and only then
    /// releases the lane). Shared by every shutdown arm below.
    fn stop_owner(&mut self) {
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
    }

    /// Nonblocking terminal shutdown: a terminal poll for an already-staged
    /// completion first (settled normally when present - full settle or
    /// feedback-only trailing refusal), then stop request, finished-only
    /// reap, and retention of an unfinished exact owner for eventual reap.
    /// A dropped inflight without a terminal completion settles honestly
    /// (see the struct docs, plus the trailing refusal when no target was
    /// ever bound). Returns 0 or 1 settlements for uniform
    /// history/feedback reconciliation. Never blocks, never panics.
    pub fn shutdown(&mut self) -> Option<ShutdownOutcome> {
        // Terminal nonblocking polls before stopping: an already-staged
        // completion settles exactly like a live poll.
        if let Some(settle) = self.poll() {
            self.stop_owner();
            return Some(ShutdownOutcome::Settle(Box::new(settle)));
        }
        if let Some(refusal) = self.poll_trailing_refusal() {
            self.stop_owner();
            return Some(ShutdownOutcome::TrailingRefusal(refusal));
        }
        let inflight = self.inflight.take();
        let thread = self.thread.take()?;
        thread.request_shutdown();
        let uncertain = thread.handoff().is_busy();
        match thread.shutdown_nonblocking() {
            None => {}
            Some(thread) => {
                crate::workspace_send_native::spawn_terminal_reaper(thread);
            }
        }
        let outcome = if uncertain {
            crate::workspace_send_effect::SendEffectOutcome::PossiblyExecuted
        } else {
            crate::workspace_send_effect::SendEffectOutcome::RefusedProven
        };
        // Setter from the busyness fact, not the outcome label: an owner
        // that may still actuate throws (unknown); a certainly-idle lane
        // refused (nothing could have posted). No reads exist on this path.
        let setter = if uncertain {
            tiler_core::world_report::FrameSetter::Threw
        } else {
            tiler_core::world_report::FrameSetter::Refused
        };
        match inflight {
            Some(SendInflight::Pinned(pinned)) => {
                Some(ShutdownOutcome::Settle(Box::new(SendSettle {
                    target: pinned.target.clone(),
                    display: pinned.display.clone(),
                    from: pinned.from,
                    follow: pinned.target.follow(),
                    outcome,
                    setter,
                    follow_view_operation: pinned.follow_view_operation.clone(),
                    trailing: None,
                    pre_member: None,
                    post_member: None,
                    pre_view: None,
                    post_view: None,
                    baseline: pinned.baseline,
                })))
            }
            Some(SendInflight::Trailing(trailing)) => {
                Some(ShutdownOutcome::TrailingRefusal(TrailingRefusal {
                    operation_id: trailing.request.operation_id.clone(),
                    setter,
                }))
            }
            None => None,
        }
    }
}

/// One settled trailing refusal for feedback-only reconciliation: the
/// operation identity plus the owner's raw setter disposition (`Refused`
/// when preparation certainly never actuated, `Threw` when it may still be
/// running). No target was ever bound (preparation refused or was cancelled
/// first), so there is no history to note and no proof to report - the
/// transfer settles or holds under its identity alone. The setter is the
/// actuation-uncertainty truth, never derived from an arrival verdict
/// (there is no arrival on this path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingRefusal {
    pub operation_id: String,
    pub setter: tiler_core::world_report::FrameSetter,
}

/// One terminal shutdown settlement for uniform reconciliation: either a
/// full settle (pinned sends and owner-bound trailing executions) or a
/// feedback-only trailing refusal (preparation never bound a target).
/// The settle boxes (its pinned target dominates the size) so the enum
/// stays compact.
#[derive(Debug, Clone)]
pub enum ShutdownOutcome {
    Settle(Box<SendSettle>),
    TrailingRefusal(TrailingRefusal),
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
/// operation identity, plus the route (`unselected` here - the owner
/// chooses bridged-first with drag fallback later), the `admission` phase
/// token, the pinned topology revision, and the process-common monotonic
/// clock (`now_ms` from the eligibility origin - comparable across the
/// effect-owner leg lines, unlike leg-relative `t_ms`). IDs only (never
/// titles, content, or paths). Pure (the caller emits on the submitted path
/// only); streams live under `--stream` like every stderr line, before any
/// teardown.
// The admission join needs subject, target, source, display, op, revision,
// and both clocks together; callers pass IDs, never user text.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn format_send_admitted_line(
    action: &str,
    subject: &str,
    target: u64,
    from: u64,
    display: &str,
    op: &str,
    revision: Option<u64>,
    now_ms: u64,
) -> String {
    format!(
        "tiler-macos send admitted action={action} subject={subject} target={target} from={from} display={display} op={op} route=unselected phase=admission revision={revision:?} now_ms={now_ms} submitted complete=false"
    )
}

/// Human-short source token for one send command: explicit outputs render
/// the output id, implicit sources render their resolution intent. Pure
/// (the refusal line carries it; the core owns the actual resolution).
#[must_use]
pub(crate) fn describe_send_source(source: &SendSource) -> String {
    match source {
        SendSource::Explicit(output) => output.0.clone(),
        SendSource::HomeThenAction => "home-action".to_owned(),
        SendSource::HomeThenCachedLive => "home-cached-live".to_owned(),
    }
}

/// Plain send-command shape for the refusal line: `(subject, index,
/// follow, source)`. Numbered sends carry the positional index, relative
/// sends carry the signed delta, direct sends carry `direct`, directional
/// sends carry `directional`; non-send commands carry the `none` fallback
/// (the dispatch site never renders those: only send-shaped refusals log).
/// Pure (the dispatch site renders; tests pin the vocabulary).
#[must_use]
pub(crate) fn describe_send_command(command: &WorldCommand) -> (String, String, bool, String) {
    match command {
        WorldCommand::SendNumbered {
            window,
            source,
            index,
            follow,
        } => (
            window.0.clone(),
            index.to_string(),
            *follow,
            describe_send_source(source),
        ),
        WorldCommand::SendRelative {
            window,
            source,
            delta,
            follow,
        } => (
            window.0.clone(),
            format!("relative{delta:+}"),
            *follow,
            describe_send_source(source),
        ),
        WorldCommand::Send {
            window,
            target_output,
            target_workspace,
            follow,
        } => (
            window.0.clone(),
            "direct".to_owned(),
            *follow,
            format!("{}/{}", target_output.0, target_workspace),
        ),
        WorldCommand::SendDirectional {
            window,
            direction,
            follow,
        } => (
            window.0.clone(),
            "directional".to_owned(),
            *follow,
            direction.clone(),
        ),
        _ => ("none".to_owned(), "-".to_owned(), false, "none".to_owned()),
    }
}

/// One bounded metadata-only refusal line for a core-rejected SEND: the
/// dispatch op, the core kind, the requested send shape (subject/index/
/// follow/source — never titles, content, or paths), the correlated report
/// observation/request ids plus the command sequence the rejection
/// consumed, and the held-scope detail (`scope:reason` per unreadable
/// scope plus `roster:{pid}:{cause}` per incomplete roster, `-` when
/// nothing is held). Pure (the dispatch site emits on the rejected path
/// for send-shaped commands only, so accepted traffic and non-send
/// refusals pay nothing); streams live under `--stream` like every stderr
/// line, before any teardown.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(crate) fn format_send_refused_line(
    op: &str,
    kind: &str,
    subject: &str,
    index: &str,
    follow: bool,
    source: &str,
    observation_seq: u64,
    request_seq: u64,
    request_id: &str,
    command_seq: Option<u64>,
    held: &str,
) -> String {
    format!(
        "tiler-macos send refused op={op} kind={kind} subject={subject} index={index} follow={follow} source={source} obs={observation_seq} req={request_seq} req_id={request_id} command_seq={command_seq:?} held={held}"
    )
}
