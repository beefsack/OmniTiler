//! Pure core derivation of per-window classification from raw nullable
//! [`WindowFact`](crate::world_report::WindowFact) inputs.
//!
//! The adapter reports raw native facts with unknowns preserved; the core
//! owns every membership verdict, explicit overrides, born latches, and
//! last-known frames. This module maps one raw fact to one
//! [`ClassifiedWindow`] carrier. The session layer decides admission from
//! these carriers (explicit tile/float overrides and retained tile state
//! ride the inputs; resizable is carried without ever becoming an admission
//! input).
//!
//! Predicate states per input: `Applicable` (supported and known),
//! `Skipped` (listed in `unavailable_facts`: the gate does not apply and a
//! `false` default feeds only that gate, never a verdict), and `UnknownHold`
//! (supported but null: the dependent classification holds narrowly, never
//! a silent default). Available `managed=false` excludes host-unmanaged
//! candidates; supported-null `managed` holds. `privileged`, `toolWindow`,
//! and `activationInhibited` known-true refuse on the safety gate;
//! supported-null holds. `owned` known-true refuses alone (existing host
//! safety gate); supported-null `owned` holds.
//! Transient/modal and dialog/sheet kinds float; a transient/modal
//! unknown holds only while no independent float origin (the other
//! known-true predicate, or a dialog/sheet kind) already proves float,
//! and skipped inputs default false (tile). A null layer holds only the
//! non-float candidate the layer gate would actually judge.
//!
//! Shell rules (no Settings exemption, no metadata-self heuristic):
//! known-unowned generic Win32 dialog class `#32770` refuses membership
//! (never a blanket type-dialog rule); exactly the five unconditional
//! shell classes refuse; popup, product-hide-only, and generic-XAML
//! shell-host surfaces are flagged border-only (signal only: never
//! membership, floating, or fit-exclusion input; consumed by the
//! foreground-border policy). A generic-XAML surface whose executable
//! is supported-but-unreadable fails the signal closed (border hides)
//! while membership proceeds; only a known non-shell executable shows
//! the border. Ordinary explorer file browsers and foreign
//! generic-XAML windows are not excluded by resemblance. Activation policy
//! `accessory`/`prohibited` refuses; a nonordinary (nonzero) layer refuses
//! only after the recognized float exceptions. Process identity excludes
//! only on known equality with this connection's own process id; a
//! separate process of the same executable is never excluded by identity
//! alone, and Settings is ordinary on every platform.

use crate::connection::FactName;
use crate::size_hints::{FixedSizePredicate, is_fixed_size_with};
use crate::world_report::{ActivationPolicy, WindowFact, WindowType};

/// Per-fact predicate state: whether one native input feeds its gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PredicateState {
    /// Supported and known: the gate applies to the carried value.
    Applicable(bool),
    /// Listed unavailable: the gate does not apply; `false` feeds only
    /// that gate.
    Skipped,
    /// Supported but null: the dependent classification holds.
    UnknownHold,
}

impl PredicateState {
    /// Resolve one nullable fact against the connection's unavailable list.
    fn resolve(value: Option<bool>, fact: FactName, unavailable: &[FactName]) -> Self {
        if unavailable.contains(&fact) {
            Self::Skipped
        } else {
            match value {
                Some(known) => Self::Applicable(known),
                None => Self::UnknownHold,
            }
        }
    }

    /// Known-true only when applicable and true.
    fn known_true(self) -> bool {
        matches!(self, Self::Applicable(true))
    }

    /// Gate value: applicable carries truth; skipped defaults false. Call
    /// only after ruling out [`Self::UnknownHold`].
    fn gate_or_false(self) -> bool {
        matches!(self, Self::Applicable(true))
    }
}

/// Input for one pure classification: the raw fact, the connection's
/// unavailable facts, the fixed-size predicate, the capability flags, the
/// retained/session context (retained tile membership, automatic fixed
/// float latch, explicit tile override, explicit float), and this
/// connection's own process id for own-process exclusion.
#[derive(Debug, Clone, Copy)]
pub struct ClassifyInput<'a> {
    pub fact: &'a WindowFact,
    pub unavailable: &'a [FactName],
    pub fixed_predicate: FixedSizePredicate,
    pub maximize_supported: bool,
    pub retained_tiled: bool,
    pub automatic_fixed: bool,
    pub tile_override: bool,
    pub explicit_float: bool,
    pub own_process_id: Option<&'a str>,
}

/// Core-derived per-window carrier. The first eight fields feed the
/// existing session/consume lanes under their established meanings;
/// the last four drive engine hold/exclusion behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifiedWindow {
    /// Float membership: explicit float, float origin (transient, modal,
    /// dialog/sheet kind), or fixed-size admission float.
    pub floating: bool,
    /// Excluded from tiling fit: safety/managed/shell/dialog exclusion
    /// or a held classification. Never set by the border-only signal.
    pub fit_excluded: bool,
    /// Automatic fixed-size float admission origin.
    pub fixed_auto: bool,
    /// Explicit tile override (or retained tile) suppresses the fixed
    /// float.
    pub fixed_suppress: bool,
    /// Fullscreen overlay: only `Some(true)` sets.
    pub overlay_fullscreen: bool,
    /// Maximized overlay: only `Some(true)` sets.
    pub overlay_maximized: bool,
    /// Sticky: only `Some(true)` sticks.
    pub sticky: bool,
    /// Hidden: minimized or host-hidden known-true.
    pub hidden: bool,
    /// A supported-but-unknown necessary input holds the dependent
    /// classification: the engine keeps the retained home with no managed
    /// geometry.
    pub hold: bool,
    /// Membership refusal: never a tile or float member.
    pub excluded: bool,
    /// Own-process window: known process equality with this connection.
    /// The engine drops these facts before closes.
    pub own_excluded: bool,
    /// Generic-XAML shell-host surface signal. Separate from membership
    /// and fit: never tiled, floated, or excluded by this flag. The
    /// foreground-border policy consumes it for fact-backed foregrounds.
    pub border_only: bool,
}

/// Exact unconditional shell classes: never tile targets, class alone
/// suffices. Exactly the five `is_shell_class` members; popup and
/// product-hide-only classes affect the border signal only, never
/// membership.
const UNCONDITIONAL_SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "DV2ControlHost",
];

/// Exact popup shell classes: border-only (task view, switcher, tray
/// flyout, shell flyout), never membership exclusions.
const POPUP_SHELL_CLASSES: &[&str] = &[
    "MultitaskingViewFrame",
    "TaskSwitcherWnd",
    "NotifyIconOverflowWindow",
    "Shell_Flyout",
];

/// Product-hide shell classes (the five unconditional members plus the
/// product-hide-only `MSCTFIME UI` and `SystemTray_Main`): border-only,
/// never membership exclusions.
const PRODUCT_HIDE_BORDER_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "DV2ControlHost",
    "MSCTFIME UI",
    "SystemTray_Main",
];

/// Generic Win32 dialog class: known-unowned top-level dialogs never take
/// membership. Known-owned ones refuse on the owned-alone gate instead.
const GENERIC_DIALOG_CLASS: &str = "#32770";

/// Generic-XAML island shell classes: flagged border-only when the
/// executable basename is a shell experience host, never a membership
/// or fit exclusion.
const XAML_SHELL_CLASSES: &[&str] = &["Windows.UI.Core.CoreWindow", "XamlExplorerHostIslandWindow"];

/// Shell experience host basenames (lowercase; Windows values already
/// normalize): only these make a generic-XAML surface border-only.
const XAML_SHELL_BASENAMES: &[&str] = &[
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "searchhost.exe",
    "searchui.exe",
    "widgets.exe",
    "textinputhost.exe",
];

/// Executable basename on either slash; `None` when unknown.
fn executable_basename(executable: &Option<String>) -> Option<String> {
    executable.as_deref().map(|path| {
        path.rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
    })
}

/// Pure classification of one raw window fact. Exclusion (fail closed)
/// takes precedence over hold; hold takes precedence over float/tile.
#[must_use]
pub fn classify_window(input: ClassifyInput<'_>) -> ClassifiedWindow {
    let fact = input.fact;
    let unavailable = input.unavailable;
    let skipped = |name: FactName| unavailable.contains(&name);

    let overlay_fullscreen = fact.fullscreen == Some(true);
    let overlay_maximized = fact.maximized == Some(true);
    let sticky = fact.sticky == Some(true);
    let hidden = fact.minimized == Some(true) || fact.host_hidden == Some(true);

    // Own-process exclusion needs known equality on both sides; a
    // supported-but-unreadable window process id holds instead.
    let own_known = input.own_process_id.is_some();
    let process_supported = !skipped(FactName::ProcessId);
    let own_excluded = own_known
        && process_supported
        && fact
            .process_id
            .as_deref()
            .is_some_and(|pid| input.own_process_id.is_some_and(|own| own == pid));

    // Nullable predicate states for the membership gates.
    let managed = PredicateState::resolve(fact.managed, FactName::Managed, unavailable);
    let privileged = PredicateState::resolve(fact.privileged, FactName::Privileged, unavailable);
    let tool_window = PredicateState::resolve(fact.tool_window, FactName::ToolWindow, unavailable);
    let activation_inhibited = PredicateState::resolve(
        fact.activation_inhibited,
        FactName::ActivationInhibited,
        unavailable,
    );
    let owned = PredicateState::resolve(fact.owned, FactName::Owned, unavailable);
    let transient = PredicateState::resolve(fact.transient, FactName::Transient, unavailable);
    let modal = PredicateState::resolve(fact.modal, FactName::Modal, unavailable);

    // Dialog kind drives the float origin below.
    let dialog_kind = matches!(
        fact.window_type,
        Some(WindowType::Dialog) | Some(WindowType::Sheet)
    );

    // Exclusion gates on known facts (fail closed, precedence over hold).
    let mut excluded = own_excluded;
    if matches!(managed, PredicateState::Applicable(false)) {
        excluded = true;
    }
    if privileged.known_true() || tool_window.known_true() || activation_inhibited.known_true() {
        excluded = true;
    }
    if !skipped(FactName::ActivationPolicy)
        && matches!(
            fact.activation_policy,
            Some(ActivationPolicy::Accessory) | Some(ActivationPolicy::Prohibited)
        )
    {
        excluded = true;
    }
    if let Some(class) = fact.class.as_deref() {
        if UNCONDITIONAL_SHELL_CLASSES.contains(&class) {
            excluded = true;
        }
        // Known-unowned generic dialogs refuse membership; known-owned
        // ones refuse on the owned-alone gate below, and unknown-owned
        // ones hold there.
        if class == GENERIC_DIALOG_CLASS && owned == PredicateState::Applicable(false) {
            excluded = true;
        }
    }
    // Owned alone excludes (existing host safety gate, no marker needed).
    if owned == PredicateState::Applicable(true) {
        excluded = true;
    }

    // Border-only shell signal: the five unconditional members, the
    // popup four, and the product-hide-only classes are border-only by
    // class alone; generic-XAML islands additionally need a shell
    // experience host basename, or a supported-but-unreadable executable
    // (the shell-host match is undecidable, so the signal fails closed).
    // Never a membership, floating, or fit-exclusion input: the
    // foreground-border policy consumes it.
    let executable_supported = !skipped(FactName::Executable);
    let border_only = fact.class.as_deref().is_some_and(|class| {
        PRODUCT_HIDE_BORDER_CLASSES.contains(&class)
            || POPUP_SHELL_CLASSES.contains(&class)
            || (XAML_SHELL_CLASSES.contains(&class)
                && (executable_basename(&fact.executable)
                    .as_deref()
                    .is_some_and(|base| XAML_SHELL_BASENAMES.contains(&base))
                    || (executable_supported && fact.executable.is_none())))
    });

    // Float origins (transient, modal, dialog/sheet kind). Evaluated
    // before the layer gate so recognized float exceptions precede it.
    let float_member = transient.gate_or_false() || modal.gate_or_false() || dialog_kind;

    // Nonordinary layer refuses only after the float origins above.
    if !float_member && !skipped(FactName::Layer) && fact.layer.is_some_and(|layer| layer != 0) {
        excluded = true;
    }

    if excluded {
        return ClassifiedWindow {
            floating: false,
            fit_excluded: true,
            fixed_auto: false,
            fixed_suppress: false,
            overlay_fullscreen,
            overlay_maximized,
            sticky,
            hidden,
            hold: false,
            excluded: true,
            own_excluded,
            border_only,
        };
    }

    // Supported-null necessary inputs hold the dependent classification.
    // Skipped (unavailable) inputs already defaulted false in their gates
    // above and never hold. Transient/modal nulls hold only while no
    // independent float origin already proves float membership: a known
    // dialog/sheet kind or the other known-true float predicate makes
    // this null irrelevant to the float verdict (the layer gate below
    // keys off the same proven verdict). Independent safety nulls still
    // hold.
    let mut hold = false;
    for state in [
        managed,
        privileged,
        tool_window,
        activation_inhibited,
        owned,
    ] {
        if state == PredicateState::UnknownHold {
            hold = true;
        }
    }
    if transient == PredicateState::UnknownHold
        && modal != PredicateState::Applicable(true)
        && !dialog_kind
    {
        hold = true;
    }
    if modal == PredicateState::UnknownHold
        && transient != PredicateState::Applicable(true)
        && !dialog_kind
    {
        hold = true;
    }
    // Kind is required but nullable: a null kind holds the dependent
    // membership, period, independent of Class support. Only
    // normal/dialog/sheet admission is normative; utility/splash are
    // explicitly open (REQ-SPC-02) and every other nonordinary kind is
    // undecided this increment, so all of them hold narrowly rather
    // than tiling or excluding by default.
    match fact.window_type {
        None
        | Some(WindowType::Utility)
        | Some(WindowType::Splash)
        | Some(WindowType::Desktop)
        | Some(WindowType::Dock)
        | Some(WindowType::Menu)
        | Some(WindowType::Tooltip)
        | Some(WindowType::Notification)
        | Some(WindowType::Other) => {
            hold = true;
        }
        Some(WindowType::Normal) | Some(WindowType::Dialog) | Some(WindowType::Sheet) => {}
    }
    // Class feeds the shell gates; a null executable never holds
    // membership: it feeds only the border-only signal above, which fails
    // closed (shell) while membership proceeds. An unavailable executable
    // leaves the default non-shell signal.
    if !skipped(FactName::Class) && fact.class.is_none() {
        hold = true;
    }
    // Activation policy feeds its gate when supported.
    if !skipped(FactName::ActivationPolicy) && fact.activation_policy.is_none() {
        hold = true;
    }
    // Layer feeds its gate only for non-float candidates: the nonordinary
    // layer gate never applies once a recognized float origin already
    // proves float membership, so a null layer holds no proven float.
    if !skipped(FactName::Layer) && fact.layer.is_none() && !float_member {
        hold = true;
    }
    // Unknown hints hold dependent admission/geometry: never a default
    // empty hint set as truth. Hints carry no unavailable-fact name. A
    // positively proven float origin needs no constraint facts for
    // membership: explicit float, the retained automatic-fixed latch, or
    // a known transient/modal/dialog-sheet origin (under the same
    // precedence as admission below, with `fixed_now` false when hints
    // are unreadable) stays floating without inventing an empty hint
    // set. The fabricated `none` never authorizes a write.
    // Ordinary newcomers (and tile-override carriers) still hold.
    let float_without_hints = if input.explicit_float {
        true
    } else if input.tile_override {
        false
    } else if input.automatic_fixed {
        true
    } else {
        float_member
    };
    if fact.hints.is_none() && !float_without_hints {
        hold = true;
    }
    // Unknown action properties hold their dependent action: never a
    // silent false (or, for overlays, true) authority, and never an
    // admission default. Hints, fullscreen, minimized, host-hidden, and
    // sticky carry no unavailable-fact names. Maximized is
    // capability-gated: `maximized: null` with `maximize: false` (macOS
    // provisional contract425) must not hold membership; the unknown
    // overlay then gates only geometry/clear behavior downstream, never
    // admission here. Full action-specific refinement is deferred.
    if fact.fullscreen.is_none()
        || fact.minimized.is_none()
        || fact.host_hidden.is_none()
        || fact.sticky.is_none()
    {
        hold = true;
    }
    if fact.maximized.is_none() && input.maximize_supported {
        hold = true;
    }
    // Supported-but-unreadable process identity holds: own-exclusion
    // cannot be decided.
    if process_supported && own_known && fact.process_id.is_none() {
        hold = true;
    }

    if hold {
        return ClassifiedWindow {
            floating: false,
            fit_excluded: true,
            fixed_auto: false,
            fixed_suppress: false,
            overlay_fullscreen,
            overlay_maximized,
            sticky,
            hidden,
            hold: true,
            excluded: false,
            own_excluded,
            border_only,
        };
    }

    // Fixed-size admission is predicate-only (resizable is never an
    // admission input); the retained/session context enforces
    // admission-only. The automatic fixed float latch survives losing
    // hints or a predicate change: explicit float wins, then the explicit
    // tile override, then the latch, then retained tile, then newcomer
    // predicate admission.
    let fixed_now = fact
        .hints
        .is_some_and(|hints| is_fixed_size_with(hints, input.fixed_predicate));
    let (floating, fixed_auto, fixed_suppress) = if input.explicit_float {
        (true, false, false)
    } else if input.tile_override {
        (false, false, true)
    } else if input.automatic_fixed {
        (true, true, false)
    } else if input.retained_tiled {
        // D2 pin is fixed-hint admission-only: a genuine float origin
        // (transient/modal/dialog-sheet) still floats, never erased by
        // the retained tile. Explicit override and automatic latch keep
        // their priority above.
        if float_member {
            (true, false, false)
        } else {
            (false, false, fixed_now)
        }
    } else {
        let float_now = float_member || fixed_now;
        (float_now, fixed_now && !float_member, false)
    };
    // `fixed_auto` marks the automatic fixed-size origin only (never an
    // explicit or transient float); `fixed_suppress` marks the
    // retained-tile/explicit-tile suppression of that origin. The latch
    // keeps its origin mark without current fixed proof; every other
    // origin still needs it.
    let latched = input.automatic_fixed && !input.explicit_float && !input.tile_override;
    let fixed_auto = fixed_auto && (fixed_now || latched);
    let fixed_suppress = fixed_suppress && fixed_now;

    ClassifiedWindow {
        floating,
        fit_excluded: false,
        fixed_auto,
        fixed_suppress,
        overlay_fullscreen,
        overlay_maximized,
        sticky,
        hidden,
        hold: false,
        excluded: false,
        own_excluded,
        border_only,
    }
}

/// Convenience classification for tests and session lanes that carry no
/// session context: newcomer admission with no overrides and no known
/// own process.
#[must_use]
pub fn classify_newcomer(
    fact: &WindowFact,
    unavailable: &[FactName],
    fixed_predicate: FixedSizePredicate,
) -> ClassifiedWindow {
    classify_window(ClassifyInput {
        fact,
        unavailable,
        fixed_predicate,
        maximize_supported: true,
        retained_tiled: false,
        automatic_fixed: false,
        tile_override: false,
        explicit_float: false,
        own_process_id: None,
    })
}
