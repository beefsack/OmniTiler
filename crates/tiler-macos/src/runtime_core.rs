#![forbid(unsafe_code)]

//! Phase 1 core adapter: pure retained observation state plus the shared
//! wire-flag normalization. No threads, locks, or sinks: owners in
//! [`crate::runtime_thread`] own these values and drive them; reports are
//! built by [`crate::world_builder`] over this retained truth.

use std::collections::BTreeMap;

use tiler_core::{
    directional::{OutputId, WindowId, WorkspaceId},
    geometry::Rect,
    session::{DomainKey, OutputDomain},
    size_hints::WindowSizeHints,
    world_report::WindowFact,
};

use crate::runtime_types::{
    NativeContext, ObservedWindow, PHASE1_GAP, ScopedWindowMeta, parse_window_id, window_id,
};

// -- Engine retained state -----------------------------------------------------

/// Core-session lifetime for one observed window, read from the last
/// applied world reply's desired rows by lifetime (never an adapter map).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct KnownState {
    pub exception: bool,
    /// Session holds a floating placement for this window (intentional
    /// ToggleFloat or lifecycle-admitted float). Born-fullscreen holds and
    /// fresh-admitted floats carry no placement — this is what lets the
    /// wire echo tell a user float (persist) from a hold exit (tile).
    pub placed: bool,
}

impl KnownState {
    /// Core-delivery lifetime memory for one window: the adapter-side
    /// replacement for direct core-session reads (the retired
    /// `WorldEngine::engine()` boundary). `rows` carries the last applied
    /// world reply's desired rows by lifetime; unknown lifetimes read
    /// first-sight. No native-to-logical home map is kept: the
    /// member/slot/lifetime derivation that fed the retired born-hold
    /// gate duplicated core-owned policy. The core holds truthful
    /// fullscreen arrivals slotless itself (D5); the adapter keeps only
    /// the float echo below.
    ///
    /// Field mapping from the core's own delivery:
    /// - `exception`/`placed`: the row's float echo and effective frame
    ///   (the retained target or session placement the core publishes
    ///   for floats).
    pub(crate) fn of_reply(
        rows: &BTreeMap<String, tiler_core::world_report::WindowDesire>,
        id: &WindowId,
    ) -> Self {
        let Some(row) = rows.get(&id.0) else {
            return Self::default();
        };
        Self {
            exception: row.floating,
            placed: row.floating && row.effective_frame.is_some(),
        }
    }
}

/// Wire flags for one observed window: the truthful classifier facts plus
/// the intentional-float echo over core-session lifetime. Fullscreen and
/// hints always ride truthfully and are NOT decided here.
///
/// - Born fullscreen arrives truthfully with no synthetic floating: the
///   core holds first-seen fullscreen windows slotless itself (D5), and a
///   retained fullscreen transition keeps its reserved slot in plan truth
///   with no geometry writes. The adapter keeps no slot, member, or
///   lifetime maps for this decision.
/// - Retained intentional floats with a session placement
///   re-assert across observations: macOS has no AX-level float state, so
///   without this echo the next converge would read the floated window as
///   tiled and revert the exception (KWin/Windows parity, where the
///   compositor reports the float natively). Placement-less holds (born
///   fullscreen, fresh-admitted floats) still report truthfully, so hold
///   exits tile. Never gains provenance: `fixed_auto` is always false, the
///   core owns the automatic marker from admission hints.
/// - Classified floats ride truthfully; first-sight fixed hints ride as
///   plain tiles on the wire and the core admits them automatic (D1/D8).
#[must_use]
pub(crate) fn normalize_wire_flags(
    floating_cls: bool,
    fullscreen: bool,
    known: &KnownState,
) -> (bool, bool) {
    if known.exception && !fullscreen && !floating_cls && known.placed {
        return (true, false);
    }
    (floating_cls, false)
}

pub(crate) struct RetainedApp {
    pub(crate) generation: u64,
    pub(crate) seq_last: u64,
    pub(crate) cmd_counter: u64,
    pub(crate) windows: Vec<ObservedWindow>,
    pub(crate) focused_wid: Option<u32>,
}

/// Native-mode explicit minimized lifetime from classifier exclusions,
/// keyed by runtime window id (`"<pid>_<wid>"`). Refreshed per scoped
/// observation from last-known homing; reported as an explicit minimized
/// [`WindowFact`] (never silent absence, never inferred close).
/// Replaces the retired shared `MinimizedEntry`: the minimized home is
/// native vocabulary (output plus native Space id); the core derives the
/// logical homing from its own adoption mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MinimizedRecord {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub sticky: bool,
}

#[derive(Default)]
pub(crate) struct RetainedCore {
    pub(crate) apps: BTreeMap<u32, RetainedApp>,
    /// Observed focus truth: set/cleared only by typed AX observations,
    /// never by plans. A dispatched (planned) focus that is dropped or never
    /// observed must not advance this; input acts on this subject.
    pub(crate) observed_focus: Option<WindowId>,
    pub(crate) focus_seq: u64,
    /// Native-mode window→domain homing, keyed by runtime window id
    /// (`"<pid>_<wid>"`). Refreshed on every scoped observation (members
    /// only) and pruned per reporting PID, so input subjects always resolve
    /// against current homing — never a stale domain.
    pub(crate) window_domains: BTreeMap<String, DomainKey>,
    /// Native-mode topology revision of the last dispatched report (`0`
    /// until the first). Fences drag-drop press revisions against
    /// mid-gesture resnapshots.
    pub(crate) native_revision: u64,
    /// Native-mode known domains (key plus authoritative bounds) for the
    /// read-only occupancy grouping and minimized-home validity. Refreshed
    /// from the dispatch context on every world report.
    pub(crate) known_domains: BTreeMap<DomainKey, OutputDomain>,
    /// Native-mode explicit minimized lifetimes from classifier exclusions,
    /// keyed by runtime window id (`"<pid>_<wid>"`). Refreshed per scoped
    /// observation from last-known homing; reported as explicit world
    /// minimized entries (never silent absence, never inferred close).
    pub(crate) minimized: BTreeMap<String, MinimizedRecord>,
    /// Keyboard-resize repeat cursor (Windows `ResizeRepeat` parity,
    /// macOS-local): the press index for the next resize dispatch. Key-up
    /// never resets it — like KDE/Windows, only a focused-window, direction,
    /// or mode mismatch restarts the shared 12/14/16/18/20px schedule (see
    /// [`resize_repeat_next`]). No fingerprint input, matching the Windows
    /// rows (an own-write fingerprint reset would restart the mandated
    /// schedule after the first press; external drift fails closed at the
    /// Engine and the next press restarts).
    pub(crate) resize_repeat: Option<ResizeRepeat>,
}

/// Exact KDE `requestResize` / Windows `ResizeRepeat` repeat tracker
/// (portable pure step, macOS-local copy — no shared helper exists in
/// `tiler-core`, and the Windows rows are local too): the press index for
/// one keyboard-resize dispatch plus the stored repeat state for the next
/// press. Continues only when the focused window, direction, and mode all
/// match the stored press; any focus/direction/mode change restarts at 0.
/// Wire tokens throughout (`left`/`right`/`up`/`down`,
/// `outwards`/`inwards`) so the engine dispatcher shares this exact rule
/// with no duplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResizeRepeat {
    pub focused: String,
    pub direction: String,
    pub mode: String,
    pub next: u32,
}

/// Next keyboard-resize press index plus the stored state for the following
/// press. See [`ResizeRepeat`] for the exact continuation rule.
#[must_use]
pub(crate) fn resize_repeat_next(
    prev: Option<ResizeRepeat>,
    focused: &str,
    direction: &str,
    mode: &str,
) -> (u32, ResizeRepeat) {
    if let Some(state) = prev
        && state.focused == focused
        && state.direction == direction
        && state.mode == mode
    {
        let index = state.next;
        return (
            index,
            ResizeRepeat {
                focused: state.focused,
                direction: state.direction,
                mode: state.mode,
                next: index.saturating_add(1),
            },
        );
    }
    (
        0,
        ResizeRepeat {
            focused: focused.to_owned(),
            direction: direction.to_owned(),
            mode: mode.to_owned(),
            next: 1,
        },
    )
}

/// Clear observed focus when it points into `pid` (authoritative loss:
/// terminate, relaunch, or an observation that no longer reports it).
pub(crate) fn clear_focus_into(retained: &mut RetainedCore, pid: u32) {
    let inside = retained
        .observed_focus
        .as_ref()
        .and_then(|w| parse_window_id(&w.0))
        .is_some_and(|(fpid, _)| fpid == pid);
    if inside {
        retained.observed_focus = None;
    }
}

/// Adapter-side core-delivery lifetime memory for wire-flag
/// normalization: the last applied world reply's desired rows by lifetime.
/// A live reply lookup, never a durable host table: rebuilt from each
/// applied reply, empty before the first. No native-to-logical home map:
/// it existed only for the retired born-hold member/slot derivation.
#[derive(Debug, Clone, Default)]
pub(crate) struct LifetimeMemory {
    pub rows: BTreeMap<String, tiler_core::world_report::WindowDesire>,
}

impl LifetimeMemory {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Rebuild from one applied reply: desired rows by lifetime.
    pub(crate) fn from_reply(reply: &tiler_core::world_report::WorldReply) -> Self {
        let mut memory = Self::default();
        for row in &reply.windows {
            memory.rows.insert(row.lifetime.0.clone(), row.clone());
        }
        memory
    }
}

/// Collect one app's retained windows into v2 [`WindowFact`] rows for a
/// single native domain. Wire flags reuse [`normalize_wire_flags`] over
/// the adapter-side [`LifetimeMemory`] (last applied reply rows).
/// `fixed_auto` is always false: the core owns
/// the automatic marker from admission hints, never the adapter echo.
/// Fullscreen, hints, and the minimized flag ride truthfully; maximized is
/// never inferred; sticky needs positive all-Spaces proof (stage 2) and
/// stays false. Logical receipts stay unknown (`observed_workspace:
/// None`): the core derives logical homing from its own adoption mapping
/// over `native_workspace`.
pub(crate) fn collect_window_facts(
    facts: &mut Vec<WindowFact>,
    pid: u32,
    observed: &[ObservedWindow],
    domain: &DomainKey,
    memory: &LifetimeMemory,
) {
    for w in observed {
        let id = window_id(pid, w.wid);
        let known = KnownState::of_reply(&memory.rows, &WindowId(id.clone()));
        let rect = Rect {
            x: w.x,
            y: w.y,
            w: w.w,
            h: w.h,
        };
        // Wire normalization over core-delivery lifetime: fullscreen and
        // hints ride truthfully; floating carries the intentional placed
        // echo only. The macOS adapter performs no maximized
        // inference and asserts no automatic origin.
        let (floating, fixed_auto) = normalize_wire_flags(w.floating, w.fullscreen, &known);
        facts.push(WindowFact {
            window: WindowId(id),
            output: domain.output.clone(),
            rect: Some(rect),
            floating,
            fit_excluded: false,
            fullscreen: w.fullscreen,
            maximized: false,
            sticky: false,
            fixed_auto,
            fixed_suppress: false,
            hints: w.hints,
            minimized: false,
            observed_workspace: None,
            native_workspace: Some(domain.workspace.0.clone()),
        });
    }
}

/// One explicit minimized [`WindowFact`]: no frame, minimized homing, the
/// native Space as capability-gated membership evidence, no logical
/// receipt. Never silent absence, never an inferred close.
pub(crate) fn minimized_fact(entry: &MinimizedRecord) -> WindowFact {
    WindowFact {
        window: entry.window.clone(),
        output: entry.output.clone(),
        rect: None,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: true,
        observed_workspace: None,
        native_workspace: Some(entry.workspace.0.clone()),
    }
}

/// Native-mode member partition for one scoped observation: each observed
/// window homes to exactly one context domain by live Space membership —
/// never by geometry. A window is a member iff its membership is exactly
/// `[space]` for some retained domain (proven user, current or hidden, or
/// homed-but-unreadable and hidden);
/// sticky, other-Space, unknown (empty/unreadable), metadataless, and
/// non-user-Space windows omit with a count (never a close report).
/// Current members actuate; hidden members reconcile without frames or
/// focus so their layouts survive manual native switches. When several
/// domains share one Space (unified span or mid-transition duplicates),
/// membership alone cannot home: the observed rect arbitrates by greatest
/// integer overlap with a unique-maximum rule (ties, zero overlap, and
/// degenerate rects omit).
/// Returns per-domain member windows plus the omitted count.
pub(crate) fn partition_scoped_members(
    windows: &[ObservedWindow],
    meta: &[ScopedWindowMeta],
    ctx: &NativeContext,
) -> (BTreeMap<DomainKey, Vec<ObservedWindow>>, usize) {
    let spaces_of = |wid: u32| {
        meta.iter()
            .find(|m| m.wid == wid)
            .map(|m| m.spaces.as_slice())
            .unwrap_or(&[])
    };
    let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
    let mut omitted = 0usize;
    for w in windows {
        let spaces = spaces_of(w.wid);
        let homed = if spaces.len() == 1 {
            let current = spaces[0];
            let candidates: Vec<&crate::runtime_types::NativeDomain> =
                ctx.domains.iter().filter(|d| d.space == current).collect();
            match candidates.len() {
                0 => None,
                1 => Some(candidates[0]),
                _ => arbitrate_shared_domain(w, &candidates),
            }
        } else {
            None
        };
        match homed {
            Some(domain) => {
                members
                    .entry(DomainKey {
                        output: domain.output.clone(),
                        workspace: domain.workspace.clone(),
                    })
                    .or_default()
                    .push(*w);
            }
            None => omitted += 1,
        }
    }
    (members, omitted)
}

/// Greatest-overlap arbitration for a window whose current Space is homed
/// by several domains: the observed integer rect must overlap exactly one
/// candidate work area strictly more than every other (ties and zero
/// overlap omit — homing unprovable, never guessed).
fn arbitrate_shared_domain<'a>(
    w: &ObservedWindow,
    candidates: &[&'a crate::runtime_types::NativeDomain],
) -> Option<&'a crate::runtime_types::NativeDomain> {
    if w.w <= 0 || w.h <= 0 {
        return None;
    }
    let (ax0, ay0, ax1, ay1) = (
        i64::from(w.x),
        i64::from(w.y),
        i64::from(w.x) + i64::from(w.w),
        i64::from(w.y) + i64::from(w.h),
    );
    let area = |r: &Rect| {
        let (bx0, by0, bx1, by1) = (
            i64::from(r.x),
            i64::from(r.y),
            i64::from(r.x) + i64::from(r.w),
            i64::from(r.y) + i64::from(r.h),
        );
        ((ax1.min(bx1) - ax0.max(bx0)).max(0)) * ((ay1.min(by1) - ay0.max(by0)).max(0))
    };
    let mut best: Option<(&crate::runtime_types::NativeDomain, i64)> = None;
    let mut tied = false;
    for &domain in candidates {
        let overlap = area(&domain.visible);
        match best {
            None => best = Some((domain, overlap)),
            Some((_, top)) if overlap > top => {
                best = Some((domain, overlap));
                tied = false;
            }
            Some((_, top)) if overlap == top => {
                tied = true;
            }
            _ => {}
        }
    }
    match best {
        Some((domain, top)) if top > 0 && !tied => Some(domain),
        _ => None,
    }
}

/// Native-mode member windows for one domain across ALL retained apps:
/// every retained window currently homed to `key`. Observations arrive
/// per app, but each domain event must converge the complete domain
/// membership — otherwise convergence would read a partial observation as
/// mass removal.
pub(crate) fn domain_member_apps(
    retained: &RetainedCore,
    key: &DomainKey,
) -> Vec<(u32, Vec<ObservedWindow>)> {
    let mut out = Vec::new();
    for (pid, app) in &retained.apps {
        let members: Vec<ObservedWindow> = app
            .windows
            .iter()
            .filter(|w| {
                retained
                    .window_domains
                    .get(&window_id(*pid, w.wid))
                    .is_some_and(|k| k == key)
            })
            .copied()
            .collect();
        if !members.is_empty() {
            out.push((*pid, members));
        }
    }
    out
}

/// Read-only retained occupancy grouping for the menu-bar indicator: every
/// known domain (current and hidden alike, via `known_domains` so
/// memberless domains ride with empty membership) mapped to its
/// exactly-homed retained `(pid, wid)` members, sorted and deduplicated.
/// Pure over retained state — the caller joins workspace keys and current
/// flags from the shared context and proves completeness via the context
/// revision. No titles, no bundles, no paths.
pub(crate) fn retained_occupancy(retained: &RetainedCore) -> BTreeMap<DomainKey, Vec<(u32, u32)>> {
    let mut out: BTreeMap<DomainKey, Vec<(u32, u32)>> = BTreeMap::new();
    for key in retained.known_domains.keys() {
        out.entry(key.clone()).or_default();
    }
    for (pid, app) in &retained.apps {
        for w in &app.windows {
            let id = window_id(*pid, w.wid);
            if let Some(key) = retained.window_domains.get(&id) {
                out.entry(key.clone()).or_default().push((*pid, w.wid));
            }
        }
    }
    for members in out.values_mut() {
        members.sort();
        members.dedup();
    }
    out
}

/// Drop every window→domain mapping for one PID (terminate/relaunch).
/// Domain events converge full retained membership, so a dead lifetime's
/// mappings must go before planning — otherwise the closed windows would
/// ride as members instead of closes.
pub(crate) fn forget_app_members(retained: &mut RetainedCore, pid: u32) {
    let prefix = format!("{pid}_");
    retained
        .window_domains
        .retain(|id, _| !id.starts_with(&prefix));
}

/// Current engine-managed facts for one runtime window identity
/// (`"<pid>_<wid>"`): the admission-pinned app-lifetime generation, the
/// homing source domain plus whether it is currently actuated, the retained
/// fullscreen flag, the classifier-exclusion minimized lifetime, and the
/// retained topology revision the homing was derived under. Published from
/// engine-owned [`RetainedCore`] on the engine thread only — the single
/// managed-window authority for both admission pins and per-edge fresh
/// proof. Native OS reads (SLS membership, AX state, focus oracles, hit
/// tests) supplement these facts on the effect owner but never replace
/// them: an engine-unknown, removed, excluded, or drifted subject refuses
/// even when every native leg still matches. `None` when the window is
/// unknown to retained state (never guessed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineManagedFacts {
    pub generation: u64,
    pub source: DomainKey,
    pub source_current: bool,
    pub fullscreen: bool,
    pub minimized: bool,
    pub revision: u64,
}

/// Discriminated engine-facts read for the dedicated effect owner: the
/// proof verdict is identical for the last two (`None` evidence,
/// `UnknownManaged` refusal, zero posts), but the blind root names which
/// one collapsed. `Managed` carries fresh engine authority;
/// `Unmanaged` is a proven engine reply that the subject is no longer
/// managed (removed, excluded, unhomed — retained truth, never a guess);
/// `NoReply` is an unproven read (the bounded query elapsed, the inbox
/// closed, or the engine is down — never a managed verdict). Callers must
/// never substitute admission pins, static scope, or native membership on
/// either refusal — both refuse without actuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineFactsRead {
    Managed(EngineManagedFacts),
    Unmanaged,
    NoReply,
}

impl RetainedCore {
    /// Publish the current managed facts for one subject window, joined
    /// against one native context: generation, homing source, current
    /// actuation, retained fullscreen, classifier-excluded minimized
    /// lifetime, and the retained revision. The caller supplies the context
    /// the engine currently holds (same-thread read); a missing context
    /// withholds actuation (never guessed current).
    pub fn engine_managed_facts_for(
        &self,
        window: &str,
        ctx: Option<&NativeContext>,
    ) -> Option<EngineManagedFacts> {
        let (pid, wid) = parse_window_id(window)?;
        let app = self.apps.get(&pid)?;
        let source = self.window_domains.get(window)?.clone();
        let fullscreen = app.windows.iter().any(|w| w.wid == wid && w.fullscreen);
        let minimized = self.minimized.contains_key(window);
        let source_current = ctx.is_some_and(|c| c.is_current(&source));
        Some(EngineManagedFacts {
            generation: app.generation,
            source,
            source_current,
            fullscreen,
            minimized,
            revision: self.native_revision,
        })
    }
}
/// Record one scoped observation's homing: insert this PID's member
/// mappings and prune this PID's mappings for windows no longer homed
/// anywhere (authoritative loss for exactly this lifetime — other PIDs are
/// untouched, so concurrent apps never invalidate each other).
pub(crate) fn note_scoped_members(
    retained: &mut RetainedCore,
    pid: u32,
    members: &BTreeMap<DomainKey, Vec<ObservedWindow>>,
) {
    let prefix = format!("{pid}_");
    retained
        .window_domains
        .retain(|id, _| !id.starts_with(&prefix));
    for (key, windows) in members {
        for w in windows {
            retained
                .window_domains
                .insert(window_id(pid, w.wid), key.clone());
        }
    }
}

/// Record synthetic (metadataless) homing: every retained window of one
/// PID belongs to the skeleton domain. Legacy observations carry no Space
/// membership, so without this the world builder could not home, focus,
/// or close a single synthetic window.
pub(crate) fn note_synthetic_homing(
    retained: &mut RetainedCore,
    pid: u32,
    windows: &[ObservedWindow],
) {
    use tiler_core::directional::{OutputId, WorkspaceId};
    let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
    members.insert(
        DomainKey {
            output: OutputId(crate::runtime_types::PHASE1_OUTPUT.to_owned()),
            workspace: WorkspaceId(crate::runtime_types::PHASE1_WORKSPACE.to_owned()),
        },
        windows.to_vec(),
    );
    note_scoped_members(retained, pid, &members);
}

/// Refresh retained dispatch context from the report's native context:
/// known domains (key plus authoritative work-area bounds) for the
/// read-only occupancy grouping and minimized-home validity, plus the
/// topology revision fencing drag-drop press revisions. Called on every
/// world dispatch (observations, inputs, terminates) so teardown paths
/// converge too. Returns true when the revision is newly adopted (the
/// caller logs the convergence evidence).
pub(crate) fn note_dispatch_context(retained: &mut RetainedCore, ctx: &NativeContext) -> bool {
    let adopted = ctx.revision != retained.native_revision;
    retained.known_domains.clear();
    for domain in &ctx.domains {
        retained.known_domains.insert(
            DomainKey {
                output: domain.output.clone(),
                workspace: domain.workspace.clone(),
            },
            OutputDomain {
                id: domain.output.clone(),
                workspace: domain.workspace.clone(),
                // Keys drive occupancy and home validity; bounds ride the
                // authoritative work area for inspection honesty.
                bounds: domain.visible,
                gap: PHASE1_GAP,
                adjacent: BTreeMap::new(),
            },
        );
    }
    retained.native_revision = ctx.revision;
    adopted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::size_hints::WindowSizeHints;

    fn known(exception: bool) -> KnownState {
        KnownState {
            exception,
            placed: false,
        }
    }

    fn placed(exception: bool) -> KnownState {
        KnownState {
            exception,
            placed: true,
        }
    }

    #[test]
    fn wire_flags_report_fullscreen_truthfully_first_sight() {
        let unknown = KnownState::default();
        // Born fullscreen: truthful, never synthetic, never automatic. The
        // core holds first-seen fullscreen slotless itself (D5); the
        // overlay fact rides alongside for the engine write fence.
        assert_eq!(normalize_wire_flags(false, true, &unknown), (false, false));
        // Born dialog+fullscreen keeps its classifier float state.
        assert_eq!(normalize_wire_flags(true, true, &unknown), (true, false));
        // First-sight fixed rides plain on the wire: the core admits the
        // automatic float from hints (D1/D8), never the adapter echo.
        assert_eq!(normalize_wire_flags(false, false, &unknown), (false, false));
        // Classified floats ride truthfully, never gaining provenance.
        assert_eq!(normalize_wire_flags(true, false, &unknown), (true, false));
    }

    #[test]
    fn wire_flags_keep_retained_shapes_truthful() {
        // Retained tile going fullscreen: truthful, slot kept core-side.
        assert_eq!(
            normalize_wire_flags(false, true, &known(true)),
            (false, false)
        );
        // Pinned tile reporting fixed hints: stays plain (D2/D3).
        assert_eq!(
            normalize_wire_flags(false, false, &known(true)),
            (false, false)
        );
        // Still-held arrival keeps reporting truthfully while fullscreen:
        // the core keeps the held exception (fixed admission pins the
        // fullscreen observation to the exception), no synthetic echo.
        assert_eq!(
            normalize_wire_flags(false, true, &known(true)),
            (false, false)
        );
        // Retained dialog+fullscreen stays a truthful overlay exception.
        assert_eq!(
            normalize_wire_flags(true, true, &known(true)),
            (true, false)
        );
    }

    #[test]
    fn wire_flags_retain_intentional_floats_across_observations() {
        // Live defect (scoped float never stuck): macOS has no AX-level
        // float state, so a session-floated window still classifies as
        // non-floating. Without this echo the next converge reads it as
        // tiled and reverts the exception (~90 ms live: one sibling
        // expansion frame, then reconcile back to halves). A retained
        // intentional (non-fullscreen) exception WITH a
        // session placement must re-assert floating on the wire, always
        // without automatic provenance. A
        // placement-less hold (born fullscreen exit, fresh-admitted float)
        // still reports truthfully so the hold exit tiles.
        let intentional = placed(true);
        assert_eq!(
            normalize_wire_flags(false, false, &intentional),
            (true, false)
        );
        // Placement-less exceptions (hold exits) tile.
        let hold_exit = known(true);
        assert_eq!(
            normalize_wire_flags(false, false, &hold_exit),
            (false, false)
        );
        // Classified float state still rides truthfully for unknowns, and
        // ordinary retained tiles stay plain.
        let unknown = KnownState::default();
        assert_eq!(normalize_wire_flags(true, false, &unknown), (true, false));
        assert_eq!(
            normalize_wire_flags(false, false, &known(false)),
            (false, false)
        );
    }

    #[test]
    fn resize_repeat_continues_only_on_focus_direction_mode_match() {
        // Exact Windows `resize_repeat_next` parity: first press is index
        // 0 (the shared 12px step), continuation bumps the stored next,
        // and any focus/direction/mode change restarts at 0.
        let (first, state) = resize_repeat_next(None, "1001_1", "right", "outwards");
        assert_eq!(first, 0);
        assert_eq!(state.next, 1);
        let (second, state) = resize_repeat_next(Some(state), "1001_1", "right", "outwards");
        assert_eq!(second, 1);
        assert_eq!(state.next, 2);
        // Mismatches restart; key-up never enters (no reset input here).
        for (focused, direction, mode) in [
            ("1001_2", "right", "outwards"),
            ("1001_1", "left", "outwards"),
            ("1001_1", "right", "inwards"),
        ] {
            let (index, next) = resize_repeat_next(Some(state.clone()), focused, direction, mode);
            assert_eq!(index, 0, "{focused}/{direction}/{mode} restarts");
            assert_eq!(next.next, 1);
        }
        // Saturation: the index never wraps on long holds (the core caps
        // the pixel step at 20px downstream).
        let mut state = ResizeRepeat {
            focused: "1001_1".to_owned(),
            direction: "right".to_owned(),
            mode: "outwards".to_owned(),
            next: u32::MAX,
        };
        let (index, next) = resize_repeat_next(Some(state.clone()), "1001_1", "right", "outwards");
        assert_eq!(index, u32::MAX);
        assert_eq!(next.next, u32::MAX);
        state.next = 41;
        let (index, _) = resize_repeat_next(Some(state), "1001_1", "right", "outwards");
        assert_eq!(index, 41);
    }

    // -- Native member partition -------------------------------------------------

    use crate::runtime_types::{NativeContext, NativeDomain, ScopedWindowMeta};
    use tiler_core::directional::{OutputId, WorkspaceId};

    fn native_ctx() -> NativeContext {
        NativeContext {
            revision: 7,
            domains: vec![
                NativeDomain {
                    cg_id: 1,
                    uuid: Some("uuid-1".to_owned()),
                    output: OutputId("macos-display-1".to_owned()),
                    workspace: WorkspaceId("space-11".to_owned()),
                    space: 11,
                    current: true,
                    readable: true,
                    bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                    visible: Rect {
                        x: 0,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                },
                NativeDomain {
                    cg_id: 2,
                    uuid: Some("uuid-2".to_owned()),
                    output: OutputId("macos-display-2".to_owned()),
                    workspace: WorkspaceId("space-21".to_owned()),
                    space: 21,
                    current: true,
                    readable: true,
                    bounds: Rect {
                        x: 1440,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                    visible: Rect {
                        x: 1440,
                        y: 0,
                        w: 1440,
                        h: 900,
                    },
                },
            ],
            skipped: Vec::new(),
        }
    }

    fn observed(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x,
            y,
            w,
            h,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn meta(wid: u32, spaces: &[u64]) -> ScopedWindowMeta {
        ScopedWindowMeta {
            wid,
            spaces: spaces.to_vec(),
            fullscreen: false,
            minimized: false,
            hidden: false,
        }
    }

    fn retained_with(
        apps: &[(u32, Vec<ObservedWindow>)],
        homing: &[(&str, DomainKey)],
        known: &[DomainKey],
    ) -> RetainedCore {
        let mut retained = RetainedCore::default();
        for (pid, windows) in apps {
            retained.apps.insert(
                *pid,
                RetainedApp {
                    generation: 1,
                    seq_last: 1,
                    cmd_counter: 0,
                    windows: windows.clone(),
                    focused_wid: None,
                },
            );
        }
        for (id, key) in homing {
            retained
                .window_domains
                .insert((*id).to_owned(), (*key).clone());
        }
        for key in known {
            retained.known_domains.insert(
                key.clone(),
                OutputDomain {
                    id: key.output.clone(),
                    workspace: key.workspace.clone(),
                    bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 1,
                        h: 1,
                    },
                    gap: 0,
                    adjacent: BTreeMap::new(),
                },
            );
        }
        retained
    }

    #[test]
    fn retained_occupancy_groups_current_and_hidden_members() {
        use tiler_core::directional::OutputId;
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-12".to_owned()),
        };
        let d3 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-13".to_owned()),
        };
        let retained = retained_with(
            &[
                (
                    100,
                    vec![observed(1, 10, 10, 100, 100), observed(2, 10, 10, 50, 50)],
                ),
                (200, vec![observed(5, 10, 10, 100, 100)]),
                // PID 300 reports a window with no homing: unmapped, never
                // a member anywhere.
                (300, vec![observed(9, 10, 10, 100, 100)]),
            ],
            &[
                ("100_1", d1.clone()),
                ("100_2", d1.clone()),
                ("200_5", d2.clone()),
            ],
            &[d1.clone(), d2.clone(), d3.clone()],
        );
        let grouped = retained_occupancy(&retained);
        // Every known domain rides — including the memberless one (the
        // caller proves emptiness via the context revision, never here).
        assert_eq!(grouped.get(&d1), Some(&vec![(100, 1), (100, 2)]));
        assert_eq!(grouped.get(&d2), Some(&vec![(200, 5)]));
        assert_eq!(grouped.get(&d3), Some(&vec![]));
        assert_eq!(grouped.len(), 3, "no unmapped window invents a domain");
    }

    #[test]
    fn partition_homes_by_membership_never_geometry() {
        let ctx = native_ctx();
        // Geometry sits inside display 1's area for every window: only
        // membership decides (a geometry reader would home all three to
        // display 1).
        let windows = vec![
            observed(1, 10, 10, 100, 100),
            observed(2, 10, 10, 100, 100),
            observed(3, 10, 10, 100, 100),
            observed(4, 10, 10, 100, 100),
            observed(5, 10, 10, 100, 100),
        ];
        let meta = vec![
            meta(1, &[11]),
            meta(2, &[21]),
            meta(3, &[99]),
            meta(4, &[11, 21]),
            meta(5, &[]),
        ];
        let (members, omitted) = partition_scoped_members(&windows, &meta, &ctx);
        assert_eq!(omitted, 3, "other/sticky/unknown omit");
        assert_eq!(members.len(), 2);
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-21".to_owned()),
        };
        assert_eq!(
            members
                .get(&d1)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>()),
            Some(vec![1])
        );
        assert_eq!(
            members
                .get(&d2)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>()),
            Some(vec![2])
        );
    }

    #[test]
    fn partition_metadataless_windows_omit() {
        // No metadata entry at all: unknown home, omitted (never guessed
        // into a domain from geometry).
        let ctx = native_ctx();
        let windows = vec![observed(9, 10, 10, 100, 100)];
        let (members, omitted) = partition_scoped_members(&windows, &[], &ctx);
        assert!(members.is_empty());
        assert_eq!(omitted, 1);
    }

    #[test]
    fn partition_shared_space_arbitrates_by_overlap() {
        // Both domains home the same current Space: the observed rect
        // picks the unique overlap winner; ties and misses omit.
        let mut ctx = native_ctx();
        ctx.domains[1].space = 11;
        ctx.domains[1].workspace = WorkspaceId("space-11".to_owned());
        let windows = vec![
            observed(1, 100, 100, 200, 200),
            observed(2, 1500, 100, 200, 200),
            observed(3, 1340, 100, 200, 200),
            observed(4, 5000, 5000, 10, 10),
        ];
        let meta = vec![
            meta(1, &[11]),
            meta(2, &[11]),
            meta(3, &[11]),
            meta(4, &[11]),
        ];
        let (members, omitted) = partition_scoped_members(&windows, &meta, &ctx);
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let wids = |key: &DomainKey| {
            members
                .get(key)
                .map(|w| w.iter().map(|w| w.wid).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        assert_eq!(wids(&d1), vec![1]);
        assert_eq!(wids(&d2), vec![2]);
        // Window 3 straddles the seam exactly (100px each side): tie omits.
        // Window 4 overlaps nothing: omits.
        assert_eq!(omitted, 2);
    }

    #[test]
    fn member_mapping_tracks_and_prunes_per_pid() {
        let mut retained = RetainedCore::default();
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let d2 = DomainKey {
            output: OutputId("macos-display-2".to_owned()),
            workspace: WorkspaceId("space-21".to_owned()),
        };
        let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
        members.insert(d1.clone(), vec![observed(1, 0, 0, 10, 10)]);
        members.insert(d2.clone(), vec![observed(2, 0, 0, 10, 10)]);
        // Retained app windows ride alongside the mapping (the engine
        // stores member windows per app; the mapping homes them).
        retained.apps.insert(
            1001,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
                focused_wid: None,
            },
        );
        note_scoped_members(&mut retained, 1001, &members);
        assert_eq!(
            domain_member_apps(&retained, &d1),
            vec![(1001, vec![observed(1, 0, 0, 10, 10)])]
        );
        // A new observation for the same PID replaces its mappings (window
        // 1 authoritative loss) without touching other PIDs.
        retained
            .window_domains
            .insert("1002_9".to_owned(), d2.clone());
        let mut next: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
        next.insert(d2.clone(), vec![observed(2, 0, 0, 10, 10)]);
        note_scoped_members(&mut retained, 1001, &next);
        assert!(domain_member_apps(&retained, &d1).is_empty());
        assert_eq!(
            retained.window_domains.get("1002_9"),
            Some(&d2),
            "other PIDs survive a PID-local re-home"
        );
        // Terminate prunes the whole PID.
        forget_app_members(&mut retained, 1001);
        assert!(!retained.window_domains.contains_key("1001_2"));
        assert!(retained.window_domains.contains_key("1002_9"));
    }

    #[test]
    fn managed_facts_publish_generation_source_and_fullscreen() {
        // The narrow managed publication: generation, homing source,
        // current actuation, retained fullscreen, classifier-excluded
        // minimized lifetime, and the retained revision — never cached
        // topology or dispatch-time re-sampling. Unknown windows refuse
        // (never guessed).
        let mut retained = RetainedCore {
            native_revision: 9,
            ..RetainedCore::default()
        };
        let d1 = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let plain = observed(1, 0, 0, 10, 10);
        let mut full = observed(2, 0, 0, 10, 10);
        full.fullscreen = true;
        retained.apps.insert(
            1001,
            RetainedApp {
                generation: 7,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![plain, full],
                focused_wid: None,
            },
        );
        let mut members: BTreeMap<DomainKey, Vec<ObservedWindow>> = BTreeMap::new();
        members.insert(
            d1.clone(),
            vec![observed(1, 0, 0, 10, 10), observed(2, 0, 0, 10, 10)],
        );
        note_scoped_members(&mut retained, 1001, &members);
        // Classifier-excluded minimized lifetime for window 1 only.
        retained.minimized.insert(
            "1001_1".to_owned(),
            MinimizedRecord {
                window: WindowId("1001_1".to_owned()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                sticky: false,
            },
        );
        let ctx = native_ctx();
        let facts = retained
            .engine_managed_facts_for("1001_1", Some(&ctx))
            .expect("homed window publishes");
        assert_eq!(facts.generation, 7);
        assert_eq!(facts.source, d1);
        assert!(facts.source_current, "homed current domain actuates");
        assert!(!facts.fullscreen);
        assert!(facts.minimized, "classifier exclusion publishes");
        assert_eq!(facts.revision, 9);
        let full_facts = retained
            .engine_managed_facts_for("1001_2", Some(&ctx))
            .expect("homed window publishes");
        assert!(full_facts.fullscreen);
        assert!(!full_facts.minimized);
        // A missing context withholds actuation without losing identity.
        let no_ctx = retained
            .engine_managed_facts_for("1001_1", None)
            .expect("identity still publishes");
        assert!(!no_ctx.source_current);
        // Unhomed, unknown-PID, and unparsable windows publish nothing.
        assert!(
            retained
                .engine_managed_facts_for("1001_9", Some(&ctx))
                .is_none()
        );
        assert!(
            retained
                .engine_managed_facts_for("9999_1", Some(&ctx))
                .is_none()
        );
        assert!(
            retained
                .engine_managed_facts_for("not-a-window", Some(&ctx))
                .is_none()
        );
    }
}
