#![forbid(unsafe_code)]

//! Phase 1 runtime public types: configuration, typed handoffs,
//! diagnostics, and outcome tokens.
//!
//! Pure data plus validation only: no threads, locks, or sinks. The
//! supervisor surface in [`crate::runtime`] re-exports every public item
//! here, so paths like `crate::runtime::RuntimeConfig` keep working.

use std::collections::BTreeMap;
use std::time::Duration;

use tiler_core::directional::{OutputId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::session::{DomainKey, DragHoverPrior};
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

use crate::classifier::ExcludeReason;
use crate::config::ProbeModifier;
use crate::focus_route::FocusRoute;
use crate::scope::AppScope;

// -- Phase-1 single-domain skeleton constants --------------------------------
//
// One fixed domain stands in for display/Space topology until platform
// policy extraction lands. These are placeholders, not policy.

/// Skeleton output identity carried into every [`CoreEvent`].
pub const PHASE1_OUTPUT: &str = "macos-main";
/// Skeleton workspace identity carried into every [`CoreEvent`].
pub const PHASE1_WORKSPACE: &str = "macos-space-1";
/// Core owner binding for the skeleton engine.
pub const PHASE1_OWNER: &str = "macos-phase-1";
/// Core generation binding for the skeleton engine.
pub const PHASE1_GENERATION: &str = "phase-1";
/// Skeleton domain bounds (adapter-normalized integer units).
pub const PHASE1_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 1920,
    h: 1080,
};
/// Skeleton inner gap.
pub const PHASE1_GAP: i32 = 8;
/// Skeleton outer gap.
pub const PHASE1_OUTER_GAP: i32 = 8;

/// Advisory per-AX-call bound. Mirrors the existing 2 s element messaging
/// timeout used by the probe AX paths; backends enforce their own finite
/// timeout and report honestly.
pub const AX_CALL_TIMEOUT: Duration = Duration::from_secs(2);

// -- Typed handoffs ------------------------------------------------------------

/// Typed incoming app lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Launch { pid: u32 },
    Terminate { pid: u32 },
}

/// One observed window: opaque IDs plus integer frame plus the normalized
/// classifier facts the core needs. No titles, no content, no PII.
///
/// `floating` carries normalized float exceptions (dialogs, sheets, modals);
/// `fullscreen` carries the native overlay flag truthfully on every shape;
/// `hints` carries the current size-hint vector for the core's
/// admission-only fixed classifier (D1/D2/D5 freshness). The wire layer
/// holds born fullscreen arrivals slotless (synthetic floating, never
/// automatic) and asserts first-sight fixed origins; retained overlays keep
/// their reserved slot/allocation in plan truth with no geometry writes.
/// Minimized, hidden-app, policy/own-bundle, and layer exclusions never
/// reach this shape: the classifier omits them from the submission instead
/// (the core folds absence into minimize/restore, never into close).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedWindow {
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub floating: bool,
    pub fullscreen: bool,
    pub hints: WindowSizeHints,
}

/// Typed AX observation for one app. `generation` is the originating app
/// lifetime minted at launch: the runtime rejects observations whose
/// generation does not match the live generation, so a late observation from
/// a previous lifetime can never be stamped with the new generation after a
/// PID relaunch. `seq` is a per-app monotonic sequence; the runtime rejects
/// anything that is not newer than the last applied sequence for the current
/// generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxObservation {
    pub pid: u32,
    pub generation: u64,
    pub seq: u64,
    pub windows: Vec<ObservedWindow>,
    pub focused_wid: Option<u32>,
}

/// Step direction for semantic input actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDirection {
    Left,
    Right,
    Up,
    Down,
}

impl InputDirection {
    /// Core direction token (`left`/`right`/`up`/`down`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Which topology the runtime actuates: the legacy single synthetic domain
/// (default, preserving every existing offline fixture) or the actual
/// multi-domain native topology supplied via
/// [`crate::runtime::RuntimeHandle::update_topology`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TopologyMode {
    /// Single fixed synthetic domain (`macos-main` / `macos-space-1`).
    #[default]
    Synthetic,
    /// Actual multi-domain topology: per-display current-user-Space domains
    /// from [`crate::topology`] snapshots. Observations and commands carry
    /// the snapshot revision; stale revisions fence instead of actuating.
    Native,
}

/// One display's tile domain in integer adapter units: the
/// `visible` work area (menu/Dock-excluded) is what the core plans into;
/// `bounds` is the full display rect for reference. Both derive from the
/// main-thread snapshot conversion; a display whose rects never converted
/// never becomes a domain (refused, never guessed).
///
/// One domain exists per retainable Space on the display (not just the
/// current one): proven user Spaces plus homed-but-unreadable ones (the
/// latter hidden only, never actuated, never called user-typed). Manual
/// native switches keep every per-(display, Space) core session alive
/// with its own independent layout, and only the `current` domain
/// actuates frames or focus. Proven non-user Spaces (fullscreen, system)
/// never become domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDomain {
    /// CG display ID (stable within the session).
    pub cg_id: u32,
    /// Exact managed display UUID for re-homing live SLS rows (exact-UUID
    /// match per yabai; `None` when the public lookup failed — re-homing
    /// then refuses rather than guessing positionally).
    pub uuid: Option<String>,
    /// Core output identity (`macos-display-{cg_id}`).
    pub output: OutputId,
    /// Core workspace identity (`space-{sid}` for the user Space).
    pub workspace: WorkspaceId,
    /// User Space ID on this display (current or hidden).
    pub space: u64,
    /// True only for the display's current user Space: the single domain
    /// per display whose plans actuate. Hidden domains reconcile
    /// membership without frames or focus.
    pub current: bool,
    /// False for homed-but-unreadable Spaces (type read gap): retained
    /// hidden with an explicit world hold and no write authority, never
    /// actuated — a transient read gap must not release its layout.
    pub readable: bool,
    /// Full display rect in integer CG top-left points.
    pub bounds: Rect,
    /// Tiling work area in integer CG top-left points.
    pub visible: Rect,
}

/// Shared native tile context: the revision-fenced per-display domains the
/// main-thread supplier derived at snapshot time. Plain data only (IDs plus
/// integer rects — no titles, no paths, no bundle strings); per-app AX
/// owners and the engine read it through an `Arc`, never mutated in place.
/// A new snapshot replaces the whole context, so readers always see one
/// consistent revision.
///
/// `domains` holds every retainable Space per display (current plus
/// hidden): each keeps its own independent core session and layout across
/// manual native switches. Only `current` domains actuate; hidden domains
/// reconcile without frames or focus, and retire only when their Space
/// leaves the display (deleted Space, removed display, or a Space that
/// newly proves non-user) — never on a mere current-Space switch or a
/// transient unreadable type read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeContext {
    /// Snapshot fingerprint revision all domains were derived under.
    pub revision: u64,
    /// Exactly-homed user-Space domains (current plus hidden), in display
    /// order (Spaces in row order within each display).
    pub domains: Vec<NativeDomain>,
    /// Displays skipped during derivation, with honest reasons. Skipped
    /// displays tile nothing; their windows never reach the engine.
    pub skipped: Vec<SkippedDisplay>,
}

/// Display skipped during derivation, with the honest reason (counts and
/// IDs only). Skipped displays tile nothing: their windows never reach the
/// engine (omission, never destruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedDisplay {
    pub cg_id: u32,
    pub reason: String,
}

impl NativeContext {
    /// Derive the tile context from a main-thread topology snapshot.
    /// `space_type` resolves a Space ID to its read-only type (`None` =
    /// unreadable); the live side injects the existing `sls_space_type`
    /// read on the main thread, tests inject scripted types. Each SID is
    /// resolved exactly once per derive: the snapshot is taken into a
    /// local table first, so the current flag and the retain/omit decision
    /// for one SID can never disagree on a transient read.
    ///
    /// Every homed Space that does not read non-user becomes one retained
    /// core domain sharing the display's work area (Space switches never
    /// move display geometry): the display's current user-typed Space
    /// actuates, everything else reconciles without frames or focus so
    /// independent layouts survive manual native switches. Proven non-user
    /// Spaces (fullscreen, system) never become domains. A homed Space
    /// whose type reads unreadable (`None`) is retained hidden and never
    /// actuates — a transient read gap must not release its layout; only
    /// actual deletion does. Deleted SIDs (absent from the row) and removed
    /// displays retire through the existing absent-key release. Displays
    /// whose rows cannot be homed exactly (or whose rects never converted)
    /// are skipped with reasons, never guessed into a domain. When no
    /// domain on the display is current, the display actuates nothing.
    /// Domains keep native row order within each display (needed by
    /// numbered mapping). Identical snapshots derive identical contexts
    /// (revision included).
    pub fn derive(
        snapshot: &crate::topology::TopologySnapshot,
        space_type: &dyn Fn(u64) -> Option<crate::ffi::SlsSpaceType>,
    ) -> (Self, Vec<SkippedDisplay>) {
        let mut domains = Vec::new();
        let mut skipped = Vec::new();
        for display in &snapshot.displays {
            let domain = (|| {
                let geometry = snapshot
                    .displays
                    .iter()
                    .find(|d| d.cg_id == display.cg_id)
                    .ok_or_else(|| format!("display {} vanished mid-derive", display.cg_id))?;
                let bounds = geometry
                    .cg_bounds
                    .and_then(|r| r.to_rect())
                    .ok_or_else(|| format!("display {} bounds unusable", display.cg_id))?;
                let visible = geometry
                    .visible
                    .and_then(|r| r.to_rect())
                    .ok_or_else(|| format!("display {} work area unusable", display.cg_id))?;
                let (spaces, current) = crate::topology::homed_spaces(snapshot, display.cg_id)
                    .ok_or_else(|| format!("display {} Space homing unprovable", display.cg_id))?;
                // Snapshot one type verdict per SID: the actuation flag and
                // the retain/omit decision below must agree even if the
                // underlying read would flap mid-derive. Row order is kept
                // (order-preserving dedup): numbered mapping needs native
                // order, not sorted order.
                let mut ordered: Vec<u64> = Vec::with_capacity(spaces.len() + 1);
                for sid in &spaces {
                    if !ordered.contains(sid) {
                        ordered.push(*sid);
                    }
                }
                if let Some(live) = current
                    && !ordered.contains(&live)
                {
                    ordered.push(live);
                }
                let mut kinds: BTreeMap<u64, Option<crate::ffi::SlsSpaceType>> = BTreeMap::new();
                for sid in ordered.iter().copied().chain(current) {
                    kinds.entry(sid).or_insert_with(|| space_type(sid));
                }
                // The current Space actuates only on a proven user verdict;
                // anything else (missing, unreadable, fullscreen/system)
                // withholds actuation while retained SIDs stay hidden.
                let current_user = current.filter(|sid| {
                    kinds
                        .get(sid)
                        .is_some_and(|kind| matches!(kind, Some(crate::ffi::SlsSpaceType::User)))
                });
                let mut per_display = Vec::new();
                for sid in ordered {
                    let actuated = Some(sid) == current_user;
                    // Homed-but-unreadable Spaces stay retained hidden with
                    // no write authority (world hold); only proven user
                    // Spaces read.
                    let readable = kinds.get(&sid).is_some_and(|kind| kind.is_some());
                    match kinds.get(&sid) {
                        // Proven non-user: never a domain (game safety).
                        Some(Some(kind)) if !matches!(kind, crate::ffi::SlsSpaceType::User) => {
                            continue;
                        }
                        // Proven user, or homed-but-unreadable (retained
                        // hidden, never actuated — never called user-typed).
                        _ => {}
                    }
                    per_display.push(NativeDomain {
                        cg_id: display.cg_id,
                        uuid: display.uuid.clone(),
                        output: OutputId(crate::topology::output_key(display.cg_id)),
                        workspace: WorkspaceId(crate::topology::workspace_key(sid)),
                        space: sid,
                        current: actuated,
                        readable,
                        bounds,
                        visible,
                    });
                }
                if per_display.is_empty() {
                    return Err(format!(
                        "display {} has no retainable Spaces (all proven non-user)",
                        display.cg_id
                    ));
                }
                Ok::<Vec<NativeDomain>, String>(per_display)
            })();
            match domain {
                Ok(per_display) => domains.extend(per_display),
                Err(reason) => skipped.push(SkippedDisplay {
                    cg_id: display.cg_id,
                    reason,
                }),
            }
        }
        (
            Self {
                revision: snapshot.revision,
                domains,
                skipped: skipped.clone(),
            },
            skipped,
        )
    }

    /// Domain keys currently actuated (one per display at most): plans for
    /// these emit frames and focus. Everything else in `domains` reconciles
    /// membership without actuation.
    #[must_use]
    pub fn current_keys(&self) -> Vec<DomainKey> {
        self.domains
            .iter()
            .filter(|d| d.current)
            .map(|d| DomainKey {
                output: d.output.clone(),
                workspace: d.workspace.clone(),
            })
            .collect()
    }

    /// True when `key` names a currently-actuated domain.
    #[must_use]
    pub fn is_current(&self, key: &DomainKey) -> bool {
        self.domains
            .iter()
            .any(|d| d.current && d.output == key.output && d.workspace == key.workspace)
    }
}

/// Per-window scoped metadata carried alongside a native snapshot. The
/// `spaces` list is the live Space membership read for the window (empty =
/// unreadable/unknown, never "no Space"); the engine admits a window only
/// on exact `[current]` membership and omits everything else. Show-state
/// flags ride along for the pre-effect recheck baseline (setters re-read
/// live; these never authorize alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedWindowMeta {
    pub wid: u32,
    pub spaces: Vec<u64>,
    pub fullscreen: bool,
    pub minimized: bool,
    pub hidden: bool,
}

/// One classifier-omitted window: identity plus the gate that fired.
/// Diagnostic only (wid plus a status name); never titles, content, or raw
/// input. The engine still sees omission only through `excluded` counts and
/// the retained-minimize path — these reasons never gate, stage, or plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExcludedOmission {
    pub wid: u32,
    pub reason: ExcludeReason,
}

/// One backend-reported scoped observation payload for a single app. Plain
/// data only; handles never escape the owning thread. `revision` is the
/// context revision read at snapshot time: the engine re-fences it against
/// the current context, so snapshot drift supersedes plans instead of
/// actuating stale domains. `focus_evidence` is stamped by the owning
/// worker from the main-thread supplier (never by the backend), so a
/// backend can never forge global focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedBackendSnapshot {
    pub windows: Vec<ObservedWindow>,
    pub focused_wid: Option<u32>,
    /// CG owner join for the focused window (observed on the owner thread
    /// alongside the snapshot); false withholds global focus, never claims.
    pub focus_cg_owned: bool,
    /// Worker-stamped focus evidence (`None` until the controller publishes
    /// any: focus stays unfocused, never guessed). Stamped from the
    /// main-thread supplier at send time with the worker's bounded
    /// selected-oracle read (`sys_focus`) over the `ax_focus` slot when the
    /// owner read succeeded, so the live AX leg is always owner-thread
    /// truth (the main thread performs no AX reads and the tile controller
    /// leaves the slot empty).
    pub focus_evidence: Option<FocusEvidence>,
    /// Bounded selected-oracle focus read taken on the owning per-app AX
    /// thread alongside this snapshot (`None` = unreadable there, never a
    /// claim). The owner never fabricates this from local state: it is a
    /// real selected SLPS+direct read (SLPS front agreeing with the
    /// workspace frontmost before+after, direct per-app focused window with
    /// CG ownership) — or nothing. The raw system-wide read stays
    /// diagnostic-only and never lands here. Manual submissions attest
    /// their own legs instead (test/controller surface); the live path
    /// always reads.
    pub sys_focus: Option<(u32, Option<u32>)>,
    /// Global-focus promotion verdict: `Some((pid, wid))` only when the
    /// selected-oracle AX leg, the app-local focused window, the CG join,
    /// and the main-thread frontmost all agree. Stamped by the worker at
    /// send time (freshest evidence); the engine promotes observed focus
    /// only on this verdict matching the reported window.
    pub global_focus: Option<(u32, u32)>,
    pub meta: Vec<ScopedWindowMeta>,
    /// Classifier-omitted window count (excluded shapes never publish as
    /// observed, so the engine cannot mistake omission for closure).
    /// Always equals `excluded_reasons.len()`; the count (not the reasons)
    /// is what the staging gate compares, so reason detail never republishes.
    pub excluded: usize,
    /// Per-omitted-window reason detail, diagnostic only. Excluded from the
    /// staging gate ([`crate::runtime_observe::scoped_snapshots_equal`]
    /// compares the count): a reason change with identical windows, meta,
    /// and count stages nothing new.
    pub excluded_reasons: Vec<ExcludedOmission>,
    pub revision: u64,
}

impl ScopedBackendSnapshot {
    /// Fail-closed mapping of a legacy (metadataless) snapshot: every
    /// window carries unknown membership, so the engine omits all of them.
    /// Used only by the default trait path; the native backend always
    /// attaches real membership.
    #[must_use]
    pub fn from_unscoped(
        windows: Vec<ObservedWindow>,
        focused_wid: Option<u32>,
        revision: u64,
    ) -> Self {
        Self {
            windows,
            focused_wid,
            focus_cg_owned: false,
            focus_evidence: None,
            sys_focus: None,
            global_focus: None,
            meta: Vec::new(),
            excluded: 0,
            excluded_reasons: Vec::new(),
            revision,
        }
    }
}

/// Main-thread-supplied global focus evidence: workspace-frontmost PID plus
/// the selected-oracle AX focus slot. Plain IDs only. The main thread fills
/// the frontmost legs and NEVER the AX leg (no AX on the controller thread):
/// the owning per-app worker stamps `ax_focus` from its bounded
/// selected-oracle read (`sys_focus`) at send time. The engine promotes an
/// app-local focused window to observed truth only on full agreement (see
/// [`crate::focus_route::global_focus_agrees`]); anything else arrives
/// unfocused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusEvidence {
    pub frontmost_pid: Option<u32>,
    pub frontmost_bundle: Option<String>,
    pub ax_focus: Option<(u32, Option<u32>)>,
}

/// Typed focus command to one app worker through its latest-wins focus
/// slot. `generation` fences PID reuse exactly like [`FrameCommand`], and
/// `revision` fences display/Space change mid-plan: a worker dispatches
/// only commands matching the live generation and revision (counted
/// otherwise, never dispatched). `route` selects the native dispatcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusCommand {
    pub pid: u32,
    pub generation: u64,
    pub request_id: u64,
    pub wid: u32,
    pub window: String,
    pub revision: u64,
    pub route: FocusRoute,
}

/// Typed semantic input action. Produced by the input thread from exact
/// chord matches on key down only; the input thread never touches the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSemanticAction {
    FocusStep {
        direction: InputDirection,
    },
    MoveStep {
        direction: InputDirection,
    },
    /// Minimal float toggle for the observed-focus window, resolved
    /// engine-side to the existing [`tiler_core::boundary::CoreCommand::ToggleFloat`]
    /// with no explicit rect. No additive core API.
    ToggleFloat,
    /// Keyboard resize step for the observed-focus window, resolved
    /// engine-side to the existing
    /// [`tiler_core::boundary::CoreCommand::Resize`] with the shared
    /// [`tiler_core::cosmic_v1::keyboard_step_px`] schedule (12px first
    /// press, then 14/16/18/20 capped) and the exact Windows/KDE repeat rule
    /// (continue only on focused-window/direction/mode match, else restart).
    /// `Outwards` grows the focused node, `Inwards` shrinks it. The
    /// `press_index` is tracked macOS-side per repeat rule; the core owns
    /// the pixel step. No additive core API.
    ResizeStep {
        direction: InputDirection,
        mode: tiler_core::contract::ResizeMode,
    },
    /// Parent split-orientation toggle for the observed-focus window,
    /// resolved engine-side to the existing
    /// [`tiler_core::boundary::CoreCommand::ToggleOrientation`]. No additive
    /// core API.
    ToggleOrientation,
    /// Numbered workspace select: `index` 1-9 selects that ordinal in the
    /// scoped ring, `index` 0 selects-or-creates the trailing empty
    /// workspace (Windows `workspace-select-N` / KDE
    /// `omnitiler-workspace-N` parity, verified against
    /// [`tiler_core::workspace::plan_trailing`]). No product execution yet:
    /// recognized by the engine and honestly refused (see
    /// [`crate::workspace_input::workspace_unsupported_reason`]).
    WorkspaceSelect {
        index: u8,
    },
    /// Numbered workspace send: `index` 1-9 sends the observed-focus
    /// window there, `index` 0 appends a new workspace for it (Windows
    /// `workspace-send-N` / KDE `omnitiler-move-workspace-N` parity).
    /// `follow` pins the intent: true follows the mover (default-bound),
    /// false stays on the source (bindable, unbound by default: Windows
    /// `stay-workspace-N` / KDE `omnitiler-stay-workspace-N` parity). No
    /// product execution yet: recognized and honestly refused.
    WorkspaceSend {
        index: u8,
        follow: bool,
    },
    /// Relative workspace select: one ordinal step in the scoped ring,
    /// never MRU (`prev` picks the direction; Windows `workspace-prev-*` /
    /// `workspace-next-*` / KDE `omnitiler-workspace-prev-*` parity). No
    /// product execution yet: recognized and honestly refused.
    WorkspaceRelative {
        prev: bool,
    },
    /// Relative workspace send: one ordinal step resolved once before
    /// transfer (`prev` picks the direction, `follow` pins the intent;
    /// stay rows bindable, unbound by default). No product execution yet:
    /// recognized and honestly refused.
    WorkspaceSendRelative {
        prev: bool,
        follow: bool,
    },
    /// Previous-view toggle: the two-view MRU flip (Windows
    /// `workspace-previous` / KDE `omnitiler-workspace-previous` parity),
    /// not ordinal traversal. No product execution yet: recognized and
    /// honestly refused.
    WorkspacePrevious,
    /// Explicit directional output send: transfer to the adjacent output's
    /// current workspace in `direction` (Windows `send-output-*` parity;
    /// the chord form rides the approved auxiliary arm). `follow` pins the
    /// intent (only follow has approved chords). No product execution yet —
    /// cross-display transfer is TBD (see
    /// [`crate::workspace_input::workspace_unsupported_reason`]).
    WorkspaceOutput {
        direction: InputDirection,
        follow: bool,
    },
    /// Meta+left drag drop: commit the press-proven `window` at the final
    /// pointer snapshot (`x`, `y`, CG top-left points) with the carried
    /// sticky hover prior. `press_revision` is the topology revision proven
    /// at press time; the worker refuses when its current revision drifted
    /// (stale pid_wid reuse never mutates) and when the window left live
    /// membership. The worker homes the subject to its current domain and
    /// dispatches [`tiler_core::boundary::CoreCommand::DragDrop`]
    /// over that domain's full membership — the same converge path as the
    /// keyboard arms, so the current Engine commits (never an independent
    /// one) and the returned plan geometry flows to the native backends
    /// through the existing desired-frame application. Same-domain only:
    /// no source pair rides along (the worker has no target-domain rows),
    /// so an outside-domain pointer resolves to `cross-domain-mismatch`
    /// and snaps back. No additive core API.
    DragDrop {
        window: String,
        x: i32,
        y: i32,
        hover_prior: Option<Box<DragHoverPrior>>,
        press_revision: u64,
    },
}

/// One pending product workspace claim awaiting the main-thread drain: the
/// armed action plus the immutable click binding that produced it. Numbered
/// tap chords bind at engine admission (the admission resolves the native
/// ordinal to its SID plus the live topology revision, exactly like the
/// send lane); menu-bar segment clicks carry the full render-time binding,
/// and the drain refuses any claim whose ordinal no longer names the expected
/// SID or whose revision drifted. Relative/previous taps ride binding-free.
/// Send-shaped claims (offline seam only --
/// product never enqueues them) carry a binding bound at admission: the
/// engine resolves the target SID plus the current topology revision at
/// admission time and pins the focused-window subject (`send_subject`)
/// with its app-lifetime generation (`send_generation`, never re-sampled
/// at dispatch) and source domain (`send_source`), so
/// the send drain revalidates the ordinal against the same generation and
/// never grabs a different window. An ordinal is therefore never interpreted
/// against a different generation or topology than the claim rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedSelect {
    pub action: InputSemanticAction,
    pub expected_sid: Option<u64>,
    pub ui_generation: u64,
    pub topology_revision: Option<u64>,
    /// Pinned focused-window subject for send-shaped claims (`"<pid>_<wid>"`,
    /// observed focus at admission, homed current, never fullscreen).
    /// Always `None` for selects (no subject) and for unbound tap claims
    /// before engine admission binds them.
    pub send_subject: Option<String>,
    /// Admission-pinned app-lifetime generation for the subject PID
    /// (from retained observation truth at admission, never re-sampled at
    /// dispatch: a PID reuse after relaunch must refuse instead of grabbing
    /// the new lifetime's window). Always `None` for selects.
    pub send_generation: Option<u64>,
    /// Admission-pinned source domain homing the subject (`window_domains`
    /// at admission). Fresh proof requires the subject still homed to this
    /// exact domain and the domain still current — never a moved or hidden
    /// domain. Always `None` for selects.
    pub send_source: Option<DomainKey>,
    /// Stable world-command operation identity (`op-<command_seq>`) minted
    /// at engine dispatch: the main-thread drain reports its native outcome
    /// under this id so the core settles the planner-owned view target
    /// (selects) or window transfer (sends) — never replayed. `None` until
    /// the engine binds the claim.
    pub operation_id: Option<String>,
}

impl ClaimedSelect {
    /// Tap/keyboard claim: no render-time binding (the engine admission
    /// binds numbered selects and offline send-shaped claims; relative and
    /// previous selects ride binding-free).
    #[must_use]
    pub const fn tap(action: InputSemanticAction) -> Self {
        Self {
            action,
            expected_sid: None,
            ui_generation: 0,
            topology_revision: None,
            send_subject: None,
            send_generation: None,
            send_source: None,
            operation_id: None,
        }
    }

    /// Menu-bar segment claim: the full immutable render-time binding rides
    /// through engine admission and the native drain.
    #[must_use]
    pub const fn indicator(
        action: InputSemanticAction,
        expected_sid: u64,
        ui_generation: u64,
        topology_revision: u64,
    ) -> Self {
        Self {
            action,
            expected_sid: Some(expected_sid),
            ui_generation,
            topology_revision: Some(topology_revision),
            send_subject: None,
            send_generation: None,
            send_source: None,
            operation_id: None,
        }
    }
}

/// Typed frame command to one app worker. `generation` fences PID reuse:
/// a worker applies only commands for the generation it was spawned with.
/// `topology_revision` fences display/Space change mid-plan: a worker
/// skips commands whose revision no longer matches the current snapshot
/// (counted, never applied). Synthetic-mode commands carry revision 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCommand {
    pub pid: u32,
    pub generation: u64,
    pub cmd_id: u64,
    pub window: String,
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub topology_revision: u64,
    /// Engine-homed target domain at plan time (`None` for legacy synthetic
    /// plans). The backend pins effects to this domain: a window that
    /// crosses displays mid-plan refuses the remaining legs instead of
    /// actuating outside its planned domain.
    pub domain: Option<DomainKey>,
}

/// Typed focus request out of the engine. `generation` fences PID reuse and
/// `request_id` is a per-runtime monotonic identity: queued requests are
/// purged on app terminate/shutdown and re-checked for freshness when
/// drained, so a stale request can never dispatch after relaunch.
/// `dispatched` means queued for the native dispatcher; `verified` is always
/// false in the skeleton — only a later [`AxObservation`] can verify focus.
/// Dispatch must never be read as arrival. `topology_revision` fences
/// display/Space change mid-plan exactly like [`FrameCommand`]: queued
/// requests from a superseded revision drain stale instead of dispatching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusRequest {
    pub pid: u32,
    pub generation: u64,
    pub request_id: u64,
    pub wid: u32,
    pub window: String,
    pub dispatched: bool,
    pub verified: bool,
    pub topology_revision: u64,
}

/// Honest AX completion: dispatch truth only (accepted, refused, timed
/// out, or partially applied). A completion never substitutes observed
/// truth, which arrives exclusively through [`AxObservation`]. Reasons carry
/// OS status names only, never user content. `PartialApplied` carries the
/// actual partial truth after the single bounded readback: which legs took
/// effect plus the readback rect (`None` when unreadable, never guessed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxCompletion {
    Applied {
        elapsed_ms: u64,
    },
    PartialApplied {
        elapsed_ms: u64,
        /// First size leg applied (Silica order runs size before position;
        /// a first-size refusal with later legs applied is still partial).
        size_first_applied: bool,
        position_applied: bool,
        size_applied: bool,
        readback_xywh: Option<(i32, i32, i32, i32)>,
        readback_unreadable: bool,
    },
    Failed {
        reason: String,
    },
    Timeout {
        timeout_ms: u64,
    },
}

/// Opaque window identity minted by the runtime: `"<pid>_<wid>"`. The
/// separator is `_` (not `:`) so the identity stays inside the core
/// opaque-ID alphabet (`is_opaque_id`: alphanumeric plus `-_.`), which the
/// focus/move paths validate before proposing.
#[must_use]
pub fn window_id(pid: u32, wid: u32) -> String {
    format!("{pid}_{wid}")
}

/// Parse a runtime-minted window identity back into `(pid, wid)`.
#[must_use]
pub fn parse_window_id(value: &str) -> Option<(u32, u32)> {
    let (pid, wid) = value.split_once('_')?;
    Some((pid.parse().ok()?, wid.parse().ok()?))
}

/// One tile input chord: an exact key plus whether the Shift arm rides
/// alongside the Ctrl-role modifier, resolving to one semantic action.
/// Matching is exact (see [`crate::input::cg_flag_bits`]): extra modifiers
/// beyond the named set fail the match. `shift` selects the move arm (the
/// Windows `FOCUS_MOVE_ROWS` Shifted shape, read-only reference); focus,
/// float, and orientation chords carry neither arm.
/// `alt` selects the resize arm (Windows `Win+Alt` grow / `Win+Shift+Alt`
/// shrink parity, read-only reference): grow chords carry Alt without Shift,
/// shrink chords carry Alt with Shift. The Alt bit sits inside the same
/// exact-match mask, so `Ctrl+H` (focus) and `Ctrl+Alt+H` (grow) are
/// distinct effective chords — never blanket keycode assumptions. Alt-as-Meta
/// collapses the resize arm onto focus/move (same effective chord), so the
/// validator below refuses that Meta for the full catalogue, exactly like
/// the pre-existing Shift-as-Meta collapse.
/// `aux` selects the workspace auxiliary arm (see
/// [`crate::input_policy::aux_flag_for`]: the Command bit under the Ctrl
/// role, else the Control bit): relative/previous/output workspace chords
/// carry it so they stay disjoint from every basic chord under every Meta.
/// Numbered workspace chords carry no auxiliary arm (plain Meta digits and
/// Meta+Shift digits); they are disjoint from the basic set because the
/// basic key set holds no digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileBinding {
    pub key_code: u32,
    pub shift: bool,
    pub alt: bool,
    pub aux: bool,
    pub action: InputSemanticAction,
}

impl TileBinding {
    pub fn validate(&self) -> Result<(), String> {
        if self.key_code > 255 {
            return Err(format!(
                "tile chord key code {} out of range 0..=255",
                self.key_code
            ));
        }
        let index = match &self.action {
            InputSemanticAction::WorkspaceSelect { index }
            | InputSemanticAction::WorkspaceSend { index, .. } => Some(*index),
            _ => None,
        };
        if index.is_some_and(|index| index > 9) {
            return Err(format!(
                "numbered workspace index {} out of range 0..=9 (1-9 plus trailing 0)",
                index.unwrap_or(99)
            ));
        }
        Ok(())
    }
}

// -- Configuration -------------------------------------------------------------

/// Runtime configuration. All queues are bounded; all waits are finite.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Bounded engine inbox depth for data jobs (observations, input
    /// actions, worker results). Full queues drop with a counted outcome.
    pub engine_queue_depth: usize,
    /// Bounded outbox depth for focus requests. Full queues drop counted.
    pub focus_outbox_depth: usize,
    /// How long `launch_app` waits for a worker to create its backend
    /// inside the owning thread before failing the launch.
    pub worker_ready_timeout: Duration,
    /// How long [`Runtime::start`] waits for the engine and input owners to
    /// signal readiness before failing the start (bounded, configurable so
    /// tests can use a short bound against a blocked factory). The same
    /// bound limits failed-start cleanup.
    pub startup_ready_timeout: Duration,
    /// Bounded async normal-log queue depth. Senders use `try_send` only:
    /// a full queue drops counted and never stalls the engine/input/apps.
    pub log_queue_depth: usize,
    /// Modifier filling the Ctrl role for input chords (default Ctrl).
    pub modifier: ProbeModifier,
    /// Key code of the exact focus-step chord.
    pub focus_key: u32,
    /// Key code of the exact move-step chord.
    pub move_key: u32,
    /// Key code of the exact float-toggle chord (minimal toggle of the
    /// observed-focus window via the existing core `ToggleFloat` command).
    pub float_key: u32,
    /// Direction carried by focus/move steps.
    pub step_direction: InputDirection,
    /// Full tile chord catalogue for the scoped-tile entry point. `None`
    /// keeps the legacy three-chord path (one focus/move/float chord plus
    /// `step_direction`); `Some` replaces it with the exact catalogue
    /// (focus/move/float arms, per-chord directions). No other behavior
    /// changes; validation requires a non-empty duplicate-free table.
    pub tile_bindings: Option<Vec<TileBinding>>,
    /// Scoped-tile bundle allowlist. Empty disables scope enforcement
    /// (legacy synthetic fixtures); non-empty enforces the explicit scope:
    /// only listed bundles actuate, Ghostty and the own bundle are rejected
    /// at validation and at the effect boundary, unknown bundles never actuate.
    pub allowed_bundles: Vec<String>,
    /// Which topology the runtime actuates (default synthetic preserves old
    /// offline fixtures; native uses supplied snapshots per display).
    pub topology_mode: TopologyMode,
    /// Which native route scoped focus requests dispatch through (default
    /// SLPS pending live agreement; public-API seam is explicit opt-in).
    pub focus_route: FocusRoute,
    /// Shared fixed-size admission predicate for the skeleton engine (D1).
    /// Both-axes default (current delivered behavior); changing it affects
    /// subsequent admissions only, never existing classifications.
    pub fixed_size_predicate: FixedSizePredicate,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            engine_queue_depth: 64,
            focus_outbox_depth: 16,
            worker_ready_timeout: Duration::from_secs(5),
            startup_ready_timeout: Duration::from_secs(5),
            log_queue_depth: crate::runtime_log::LOG_SINK_CAP,
            modifier: ProbeModifier::Ctrl,
            focus_key: crate::input::PROBE_KEY_CODE,
            move_key: 0x05,
            float_key: 0x06,
            step_direction: InputDirection::Right,
            tile_bindings: None,
            allowed_bundles: Vec::new(),
            topology_mode: TopologyMode::Synthetic,
            focus_route: FocusRoute::default(),
            fixed_size_predicate: FixedSizePredicate::BothAxes,
        }
    }
}

impl RuntimeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.engine_queue_depth == 0 {
            return Err("engine_queue_depth must be nonzero".to_owned());
        }
        if self.focus_outbox_depth == 0 {
            return Err("focus_outbox_depth must be nonzero".to_owned());
        }
        if self.focus_key == self.move_key {
            return Err("focus_key and move_key must differ".to_owned());
        }
        if self.focus_key == self.float_key || self.move_key == self.float_key {
            return Err("float_key must differ from focus_key and move_key".to_owned());
        }
        if self.startup_ready_timeout.is_zero() {
            return Err("startup_ready_timeout must be nonzero".to_owned());
        }
        if self.log_queue_depth == 0 {
            return Err("log_queue_depth must be nonzero".to_owned());
        }
        if self.focus_key > 255 || self.move_key > 255 || self.float_key > 255 {
            return Err("chord key codes must fit 0..=255".to_owned());
        }
        if let Some(bindings) = &self.tile_bindings {
            if bindings.is_empty() {
                return Err("tile_bindings must be non-empty when set".to_owned());
            }
            let mut seen = std::collections::BTreeSet::new();
            let (modifier_flags, _) = crate::input::cg_flag_bits(self.modifier);
            let aux_flags = crate::input_policy::aux_flag_for(self.modifier);
            for binding in bindings {
                binding.validate()?;
                let flags = modifier_flags
                    | if binding.shift {
                        crate::native_input::CG_SHIFT_BIT
                    } else {
                        0
                    }
                    | if binding.alt {
                        crate::native_input::CG_ALT_BIT
                    } else {
                        0
                    }
                    | if binding.aux { aux_flags } else { 0 };
                if !seen.insert((binding.key_code, flags)) {
                    return Err(format!(
                        "tile_bindings duplicates effective chord key {} shift={} alt={} aux={}",
                        binding.key_code, binding.shift, binding.alt, binding.aux
                    ));
                }
            }
        }
        AppScope {
            allowed: self.allowed_bundles.clone(),
        }
        .validate()?;
        Ok(())
    }
}

// -- Diagnostics ---------------------------------------------------------------

/// Snapshot of structured runtime counters. Bounded counts and static
/// outcome tokens only; never native identifiers beyond PIDs/WIDs already
/// present in typed handoffs, never user content.
#[derive(Debug, Clone, Default)]
pub struct RuntimeDiagnostics {
    pub engine_jobs_processed: u64,
    pub data_dropped_full: u64,
    pub result_dropped_full: u64,
    pub invalidated_on_stop: u64,
    pub frames_emitted: u64,
    pub frames_coalesced: u64,
    /// Desired frame writes skipped because the desired geometry already
    /// matches the last observed geometry (redundant once observers report
    /// back; a native clamp converges instead of rewriting endlessly).
    pub frames_redundant_skipped: u64,
    /// Desired frame writes withheld because the target window is a native
    /// overlay ([`tiler_core::window_policy::geometry_write_allowed`]
    /// refuses): retained fullscreen members keep their slot with no target
    /// geometry. Plan truth is untouched (only emission is filtered).
    pub frames_overlay_skipped: u64,
    pub focus_dispatched: u64,
    pub focus_dropped_full: u64,
    pub stale_dropped: u64,
    pub terminated_invalidated: u64,
    pub input_forwarded: u64,
    pub input_ignored: u64,
    pub input_refused_no_focus: u64,
    /// Recognized-but-unarmed workspace actions refused for missing product
    /// execution (trailing select-0, all sends/stays, all outputs — see
    /// [`crate::workspace_input::workspace_unsupported_reason`]). Counted,
    /// never dispatched. The armed select-only subset enqueues instead (see
    /// `input_select_enqueued`).
    pub input_refused_unsupported: u64,
    /// Armed select-only workspace actions accepted by the engine: one
    /// planner-owned world command per press (planner history converges)
    /// plus one pending claim handed to the main-thread tile controller.
    /// The drain reports its proven native outcome back under the claim's
    /// operation identity, settling the retained view target.
    pub input_select_enqueued: u64,
    /// Armed select-only actions refused before enqueue (no published
    /// native topology yet, or a select already claimed/pending — concurrent
    /// presses serialize/refuse instead of overlapping). Counted with
    /// static log tokens, never dispatched, never replayed.
    pub input_select_refused: u64,
    /// Backend-generated observations published to the engine by app workers
    /// (startup, streaming, and readback handoffs).
    pub observations_published: u64,
    /// Backend observation failures (snapshot/read errors that preserved
    /// retained state instead of publishing).
    pub observation_failures: u64,
    /// Unexpected end-of-stream exits (input source `None` without stop, or
    /// an engine/input owner exit that was never asked to stop). Counted and
    /// logged, never spun on.
    pub unexpected_eos: u64,
    /// Core reply outcomes by static variant token.
    pub core_replies: BTreeMap<String, u64>,
    /// Honest backend completion outcomes by static token.
    pub completions: BTreeMap<String, u64>,
    pub worker_panics_observed: u64,
    /// Returned-unclean teardown evidence (worker `shutdown()` refused).
    /// Distinct from panics, persistent like them: a reaped unclean worker
    /// still poisons the terminal shutdown.
    pub worker_unclean_observed: u64,
    /// Normal-log sink drops under saturation (bounded queue full or sink
    /// gone). The ring entry is still kept; only the async copy drops.
    pub logger_dropped: u64,
    pub apps_live: usize,
    pub log_rotated: u64,
}

/// One structured lifecycle log entry. Enough to reconstruct recovery
/// (app, generation, reference sequence/command id, outcome); no content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleLogEntry {
    pub seq: u64,
    /// Monotonic milliseconds since runtime start (ring correlation aid).
    pub timestamp_ms: u64,
    pub pid: u32,
    pub generation: u64,
    pub ref_id: u64,
    pub kind: &'static str,
    pub outcome: &'static str,
    pub count: u64,
}

/// Last desired geometry snapshot: the authoritative core plan the runtime
/// is actuating toward. Windows plus rects only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredFrame {
    pub window: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

// -- Outcomes ------------------------------------------------------------------

/// Error starting the runtime or launching an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    InvalidConfig(String),
    EngineSpawn(String),
    EngineNotReady,
    InputSpawn(String),
    InputRefused(String),
    WorkerSpawn(String),
    BackendRefused(String),
    WorkerNotReady,
    EngineQueueFull,
    PreviousGenerationRunning { pid: u32, generation: u64 },
    AdmissionClosed,
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(r) => write!(f, "invalid runtime config: {r}"),
            Self::EngineSpawn(r) => write!(f, "engine thread spawn failed: {r}"),
            Self::EngineNotReady => write!(f, "engine thread did not signal readiness"),
            Self::InputSpawn(r) => write!(f, "input thread spawn failed: {r}"),
            Self::InputRefused(r) => write!(f, "input source refused startup: {r}"),
            Self::WorkerSpawn(r) => write!(f, "app worker spawn failed: {r}"),
            Self::BackendRefused(r) => write!(f, "AX backend refused startup: {r}"),
            Self::WorkerNotReady => write!(f, "app worker did not signal readiness"),
            Self::EngineQueueFull => write!(f, "engine queue full; launch refused"),
            Self::PreviousGenerationRunning { pid, generation } => write!(
                f,
                "pid {pid} generation {generation} still executes; refusing overlap"
            ),
            Self::AdmissionClosed => write!(f, "runtime is stopped; admission closed"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Outcome of submitting data to the engine inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Accepted,
    EngineQueueFull,
    UnknownApp,
    AppTerminated,
    /// The observation carries a stale app lifetime (PID relaunched since).
    StaleGeneration,
    /// A legacy (metadataless) observation submitted in native topology
    /// mode: without per-window Space membership nothing can home, so the
    /// submission is refused instead of tiling outside the work areas.
    /// Scoped observations travel separately and are unaffected.
    RequiresScoped,
    AdmissionClosed,
}

/// Outcome of terminating an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminateOutcome {
    Terminated { generation: u64 },
    UnknownPid,
    AlreadyTerminated,
    RuntimeStopped,
}

/// Readiness of one app worker's backend (created inside the owning thread).
/// Returned by [`RuntimeHandle::worker_ready`] for the nonblocking launch
/// path; the synchronous [`RuntimeHandle::launch_app`] waits for `Ready`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerReadiness {
    UnknownPid,
    Pending,
    Ready,
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow, SlsSpaceType};
    use crate::topology;

    fn ns(cg_id: u32, uuid: &str, x: f64, y: f64, w: f64, h: f64) -> NsDisplay {
        NsDisplay {
            cg_id,
            uuid: Some(uuid.to_owned()),
            frame: DisplayRect { x, y, w, h },
            visible: DisplayRect { x, y, w, h },
            scale: 2.0,
        }
    }

    fn row(id: Option<&str>, spaces: &[u64], current: Option<u64>) -> SlsDisplayRow {
        SlsDisplayRow {
            identifier: id.map(str::to_owned),
            spaces: spaces.to_vec(),
            current,
        }
    }

    fn user_space(_: u64) -> Option<SlsSpaceType> {
        Some(SlsSpaceType::User)
    }

    #[test]
    fn derive_homes_each_display_to_actual_work_areas() {
        let snapshot = topology::build_snapshot(
            &[
                ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
                ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
            ],
            &[
                row(Some("uuid-1"), &[11], Some(11)),
                row(Some("uuid-2"), &[21], Some(21)),
            ],
            Some(true),
        );
        let (ctx, skipped) = NativeContext::derive(&snapshot, &user_space);
        assert!(skipped.is_empty());
        assert_eq!(ctx.revision, snapshot.revision);
        assert_eq!(ctx.domains.len(), 2);
        // Display 1 converts 1:1 (primary at the origin); display 2 keeps
        // its global CG x offset (work areas, never the skeleton bounds).
        assert_eq!(
            ctx.domains[0].visible,
            Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            }
        );
        assert_eq!(
            ctx.domains[1].visible,
            Rect {
                x: 1440,
                y: 0,
                w: 1440,
                h: 900,
            }
        );
        assert_eq!(ctx.domains[0].space, 11);
        assert_eq!(ctx.domains[1].space, 21);
        assert_eq!(ctx.domains[0].uuid.as_deref(), Some("uuid-1"));
        // Single-Space rows actuate their only domain.
        assert!(ctx.domains[0].current);
        assert!(ctx.domains[1].current);
        assert_eq!(ctx.current_keys().len(), 2);
        // Identical snapshots derive identical contexts.
        let (again, _) = NativeContext::derive(&snapshot, &user_space);
        assert_eq!(ctx, again);
    }

    #[test]
    fn derive_enumerates_every_user_space_with_only_current_actuated() {
        // One display with two known user Spaces and a fullscreen Space:
        // both user Spaces become retained domains sharing the display's
        // work area, only the current one actuates, and the fullscreen
        // Space never becomes a domain (game safety: nothing plans there).
        let snapshot = topology::build_snapshot(
            &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
            &[row(Some("uuid-1"), &[11, 12, 13], Some(12))],
            Some(true),
        );
        let space_type = |sid: u64| {
            Some(if sid == 13 {
                SlsSpaceType::Fullscreen
            } else {
                SlsSpaceType::User
            })
        };
        let (ctx, skipped) = NativeContext::derive(&snapshot, &space_type);
        assert!(skipped.is_empty());
        assert_eq!(ctx.domains.len(), 2);
        assert_eq!(ctx.domains[0].space, 11);
        assert!(!ctx.domains[0].current);
        assert_eq!(ctx.domains[1].space, 12);
        assert!(ctx.domains[1].current);
        // Both share the display's work area (Spaces never move displays).
        assert_eq!(ctx.domains[0].visible, ctx.domains[1].visible);
        assert_eq!(
            ctx.domains[0].workspace.0.as_str(),
            "space-11",
            "per-(display, Space) DomainKeys stay distinct"
        );
        // An unreadable type read retains hidden (never actuated, never
        // called user-typed): a transient gap must not release the layout.
        let unknown_type = |sid: u64| {
            if sid == 11 {
                None
            } else if sid == 13 {
                Some(SlsSpaceType::Fullscreen)
            } else {
                Some(SlsSpaceType::User)
            }
        };
        let (ctx, _) = NativeContext::derive(&snapshot, &unknown_type);
        assert_eq!(ctx.domains.len(), 2);
        assert_eq!(ctx.domains[0].space, 11);
        assert!(
            !ctx.domains[0].current,
            "unreadable type never actuates: {ctx:?}"
        );
        assert_eq!(ctx.domains[1].space, 12);
        assert!(ctx.domains[1].current);
        assert_eq!(
            ctx.current_keys().len(),
            1,
            "exactly the proven current actuates"
        );
        // A fullscreen current keeps hidden user domains but actuates
        // nothing on the display (no writes under the overlay).
        let fs_snapshot = topology::build_snapshot(
            &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
            &[row(Some("uuid-1"), &[11, 12, 13], Some(13))],
            Some(true),
        );
        let (ctx, _) = NativeContext::derive(&fs_snapshot, &space_type);
        assert_eq!(ctx.domains.len(), 2);
        assert!(
            ctx.domains.iter().all(|d| !d.current),
            "fullscreen current actuates nothing: {ctx:?}"
        );
        assert!(ctx.current_keys().is_empty());
    }

    #[test]
    fn derive_preserves_row_order_and_retains_unreadable_current() {
        // Nonascending native row order survives (numbered mapping needs
        // native order, not sorted order), and a current Space whose type
        // reads unreadable is retained hidden without actuating.
        let snapshot = topology::build_snapshot(
            &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
            &[row(Some("uuid-1"), &[22, 11], Some(22))],
            Some(true),
        );
        let unknown_current = |sid: u64| {
            if sid == 22 {
                None
            } else {
                Some(SlsSpaceType::User)
            }
        };
        let (ctx, skipped) = NativeContext::derive(&snapshot, &unknown_current);
        assert!(skipped.is_empty());
        assert_eq!(
            ctx.domains.iter().map(|d| d.space).collect::<Vec<_>>(),
            vec![22, 11],
            "native row order, not sorted: {ctx:?}"
        );
        assert!(!ctx.domains[0].current);
        assert!(
            !ctx.domains[1].current,
            "current unreadable actuates nothing"
        );
        assert!(ctx.current_keys().is_empty());
        // Readable again: the same row actuates its proven current in the
        // same position (no release-and-reseed across the gap).
        let (again, _) = NativeContext::derive(&snapshot, &user_space);
        assert_eq!(
            again.domains.iter().map(|d| d.space).collect::<Vec<_>>(),
            vec![22, 11]
        );
        assert!(again.domains[0].current);
        assert!(!again.domains[1].current);
        // An all-unreadable display still retains (never skips on type
        // gaps alone): skipping would release every layout.
        let all_unknown = topology::build_snapshot(
            &[ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0)],
            &[row(Some("uuid-1"), &[31], Some(31))],
            Some(true),
        );
        let (ctx, skipped) = NativeContext::derive(&all_unknown, &|_| None);
        assert!(skipped.is_empty());
        assert_eq!(ctx.domains.len(), 1);
        assert_eq!(ctx.domains[0].space, 31);
        assert!(!ctx.domains[0].current);
    }

    #[test]
    fn derive_skips_refused_displays_with_reasons() {
        // Second display's current Space reads fullscreen: it skips (tiles
        // nothing) while the healthy display still derives.
        let snapshot = topology::build_snapshot(
            &[
                ns(1, "uuid-1", 0.0, 0.0, 1440.0, 900.0),
                ns(2, "uuid-2", 1440.0, 0.0, 1440.0, 900.0),
            ],
            &[
                row(Some("uuid-1"), &[11], Some(11)),
                row(Some("uuid-2"), &[21], Some(21)),
            ],
            Some(true),
        );
        let space_type = |sid: u64| {
            Some(if sid == 11 {
                SlsSpaceType::User
            } else {
                SlsSpaceType::Fullscreen
            })
        };
        let (ctx, skipped) = NativeContext::derive(&snapshot, &space_type);
        assert_eq!(ctx.domains.len(), 1);
        assert_eq!(ctx.domains[0].cg_id, 1);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].cg_id, 2);
        assert!(!skipped[0].reason.is_empty());
        assert_eq!(ctx.skipped, skipped, "reasons ride the context");
    }
}
