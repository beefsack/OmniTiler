#![forbid(unsafe_code)]

//! Pure macOS AX/CG fact normalization for the offline observation skeleton.
//!
//! No live AX queries, no event taps, no Spaces, no TCC: every input is a
//! caller-supplied typed fact and every output is plain data. A future native
//! reader maps live attributes into [`AxFacts`]/[`CgFacts`]/[`AppFacts`];
//! this module decides only the normalized disposition, and
//! [`ClassifiedWindow::to_observed`] renders it into the existing fake engine
//! observation path ([`crate::runtime::ObservedWindow`]).
//!
//! Layering contract (deliberately thin):
//! - Per-observation normalization lives here. Lifetimes (born-fullscreen
//!   hold, first-exit classification, minimize/restore slots, fixed-admission
//!   marks) stay core-owned: the adapter passes current facts through and
//!   never re-implements them.
//! - Fixed-size evidence is the shared hints-only predicate
//!   ([`tiler_core::size_hints::is_fixed_size_with`], decisions Fixed-Size
//!   Admission D1/G-05). `AXSize` settability rides as a carried fact only:
//!   current product performs no inference from resizability, so the
//!   settable-only lane tiles normally. That behavior stays TBD for a later
//!   decision; the fact is preserved on [`ClassifiedWindow`] for it.
//! - No invented macOS AX min/max attribute is read: [`AxFacts::hints`] is an
//!   optional caller-supplied hint vector (a future native reader fills it
//!   from wherever the platform actually reports bounds). Absent hints admit
//!   normally.
//! - Dialogs and sheets float as normalized exception members. Windows-style
//!   owned-dialog exclusions are a deliberate platform difference and are NOT
//!   mapped here.
//! - The crate's own bundle is excluded outright; Ghostty is an ordinary app
//!   (no product exclusion; only live tests protect it).
//!
//! Precedence (safety first, overlays after, mirroring the Windows gate
//! order): own bundle, activation policy, minimized/hidden, then float
//! origins (sheet role, dialog subroles, modal — at any CG layer), then the
//! nonordinary-layer gate for shapes with no recognized float exception,
//! then unknown-role exclusion, then fullscreen without a float origin. The
//! overlay fact is orthogonal: float origins keep it alongside their
//! membership, so neither origin is ever dropped. Transients are never
//! reported as destroyed: exclusion means omission from the submitted
//! observation, which
//! the core folds into its minimize/restore path — never into an explicit
//! close.

use tiler_core::size_hints::WindowSizeHints;

use crate::runtime_types::ObservedWindow;

/// AX role for a normal window element.
pub const ROLE_WINDOW: &str = "AXWindow";
/// AX role for a sheet element (Silica resolves sheets to their parent; this
/// slice floats the sheet itself as a normalized exception member instead —
/// parent resolution needs live acquisition and stays later work).
pub const ROLE_SHEET: &str = "AXSheet";
/// AX subrole for a standard document window.
pub const SUBROLE_STANDARD: &str = "AXStandardWindow";
/// AX subrole for dialogs (including macOS native file pickers).
pub const SUBROLE_DIALOG: &str = "AXDialog";
/// AX subrole for floating panels and image viewers.
pub const SUBROLE_FLOATING: &str = "AXFloatingWindow";
/// AX subrole for system dialogs.
pub const SUBROLE_SYSTEM_DIALOG: &str = "AXSystemDialog";
/// AX subrole for popups, tooltips, and other non-windows.
pub const SUBROLE_UNKNOWN: &str = "AXUnknown";

/// Subroles that float as normalized exception members (never tiles, never
/// dropped). `AXStandardWindow` is the only subrole that tiles.
const FLOAT_SUBROLES: &[&str] = &[SUBROLE_DIALOG, SUBROLE_FLOATING, SUBROLE_SYSTEM_DIALOG];

/// Integer frame facts for one window (identity plus geometry only; no
/// titles, no content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFrame {
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Typed AX facts for one window. `None` role/subrole is honest unknown
/// (unreadable attribute), never a claim about the window's kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxFacts {
    /// `AXRole` (e.g. `AXWindow`, `AXSheet`).
    pub role: Option<String>,
    /// `AXSubrole` (e.g. `AXStandardWindow`, `AXDialog`).
    pub subrole: Option<String>,
    /// `AXFullScreen` state.
    pub fullscreen: bool,
    /// `AXMinimized` state.
    pub minimized: bool,
    /// `AXModal` state.
    pub modal: bool,
    /// Whether `AXSize` is settable (rift `can_resize`, glide
    /// `is_resizable`). Carried only: no admission inference is drawn from
    /// it in this slice (TBD lane).
    pub size_settable: bool,
    /// Optional caller-supplied size-hint vector. No AX min/max attribute is
    /// invented to fill it; absent hints admit normally.
    pub hints: WindowSizeHints,
}

impl AxFacts {
    /// Ordinary standard tileable window with no hints.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            role: Some(ROLE_WINDOW.to_owned()),
            subrole: Some(SUBROLE_STANDARD.to_owned()),
            fullscreen: false,
            minimized: false,
            modal: false,
            size_settable: true,
            hints: WindowSizeHints::none(),
        }
    }
}

/// Typed CG/window-server facts for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CgFacts {
    /// `kCGWindowLayer`. Normal windows report layer 0; floating panels
    /// and other levels report nonzero values. Layer alone never tiles:
    /// see the ordering in [`classify_window`].
    pub layer: i32,
    /// `kCGWindowIsOnscreen`. Informational only: off-Space windows report
    /// false while alive, so this never excludes (absence is not destruction;
    /// native Space homing is later work).
    pub onscreen: bool,
}

impl CgFacts {
    /// Normal on-screen window-server entry.
    #[must_use]
    pub const fn normal() -> Self {
        Self {
            layer: 0,
            onscreen: true,
        }
    }
}

/// Owner-app activation policy (`NSApplicationActivationPolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationPolicy {
    Regular,
    Accessory,
    Prohibited,
    /// Policy unreadable. Never used to exclude on its own: window facts
    /// govern and the app-level unknown stays tentative.
    Unknown,
}

/// Typed owner-app facts for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFacts {
    /// Application `AXHidden` state.
    pub hidden: bool,
    /// Application activation policy.
    pub activation_policy: ActivationPolicy,
    /// Whether the owner bundle is this crate's own probe bundle
    /// ([`crate::BUNDLE_ID`]). Own windows are never observed.
    pub is_own_bundle: bool,
}

impl AppFacts {
    /// Ordinary regular foreground app (covers Ghostty: no product
    /// exclusion; only live tests protect it).
    #[must_use]
    pub const fn regular() -> Self {
        Self {
            hidden: false,
            activation_policy: ActivationPolicy::Regular,
            is_own_bundle: false,
        }
    }
}

/// Why a window floats as a normalized exception member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatReason {
    /// `AXSheet` role.
    Sheet,
    /// Dialog/floating/system-dialog subrole.
    DialogSubrole,
    /// `AXModal` on an otherwise tileable window.
    Modal,
}

/// Why a window is excluded from the submitted observation (omitted, never
/// reported as closed: the core folds absence into minimize/restore).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcludeReason {
    /// The crate's own probe bundle: never observe self.
    OwnBundle,
    /// `accessory` activation policy (menu-bar extras and helpers).
    AccessoryPolicy,
    /// `prohibited` activation policy (background-only).
    ProhibitedPolicy,
    /// `AXMinimized`.
    Minimized,
    /// Owner app hidden (`AXHidden`).
    HiddenApp,
    /// Nonordinary CG layer with no recognized float exception:
    /// conservative exclusion pending an explicit TBD layer discriminator.
    /// Layer alone is never read as shell or chrome.
    NonordinaryLayer,
    /// Role present but neither `AXWindow` nor `AXSheet`, or subrole
    /// missing/unknown/other. Narrow conservative offline choice: tooltips,
    /// pills, and keyboard-switch popups must not tile. Nonstandard windows
    /// that deserve management (dialogs above) float instead; app-specific
    /// exceptions (e.g. rift's IBKR dialog rule) are deliberately not copied
    /// and stay a product decision with a discriminating sequence.
    UnknownRole,
}

/// Normalized disposition for one observed window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Ordinary tile candidate (fixed-size evidence still decided
    /// admission-side by the core from the carried hints).
    Tile,
    /// Normalized exception member: dialogs, sheets, and modals ride here.
    /// Retained members keep their reserved slot/allocation in plan truth
    /// with no geometry write; born arrivals take no slot. The independent
    /// fullscreen fact rides alongside truthfully (see
    /// [`ClassifiedWindow::fullscreen`]).
    Float(FloatReason),
    /// Native fullscreen overlay without a float origin: submitted with the
    /// fullscreen flag set and no float so the core holds born arrivals
    /// slotless. The wire layer adds synthetic floating for unknown windows
    /// (core-session lifetime); retained members stay exactly this shape so
    /// the reserved slot survives with no writes.
    Fullscreen,
    /// Omitted from the submitted observation (never a close report).
    Excluded(ExcludeReason),
}

/// One classified window: geometry plus disposition plus the carried facts
/// the engine path needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedWindow {
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub disposition: Disposition,
    /// Independent native overlay fact, always truthful from AX. Float
    /// origins (dialogs, sheets) keep it alongside their exception
    /// membership; the wire layer never drops it.
    pub fullscreen: bool,
    /// Current hint vector, carried on every admitted shape (admission-only
    /// core classification plus D5 first-exit freshness need current hints).
    pub hints: WindowSizeHints,
    /// Carried `AXSize` settability for the later TBD decision. Never
    /// consulted by classification here.
    pub size_settable: bool,
}

impl ClassifiedWindow {
    /// Render into the existing engine observation shape. `None` for
    /// excluded windows (the caller omits them from the submission).
    /// Fullscreen rides truthfully on every admitted shape; the wire hold
    /// may add synthetic floating for unknown windows later.
    #[must_use]
    pub fn to_observed(&self) -> Option<ObservedWindow> {
        let floating = match self.disposition {
            Disposition::Tile => false,
            Disposition::Float(_) => true,
            Disposition::Fullscreen => false,
            Disposition::Excluded(_) => return None,
        };
        Some(ObservedWindow {
            wid: self.wid,
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            floating,
            fullscreen: self.fullscreen,
            hints: self.hints,
        })
    }
}

/// Classify one window from its frame plus typed AX/CG/app facts. Pure:
/// no I/O, no global state, no content. See the module docs for precedence
/// and TBD lanes.
#[must_use]
pub fn classify_window(
    frame: WindowFrame,
    ax: &AxFacts,
    cg: &CgFacts,
    app: &AppFacts,
) -> ClassifiedWindow {
    let WindowFrame { wid, x, y, w, h } = frame;
    let base = |disposition| ClassifiedWindow {
        wid,
        x,
        y,
        w,
        h,
        disposition,
        fullscreen: ax.fullscreen,
        hints: ax.hints,
        size_settable: ax.size_settable,
    };
    // 1. Never observe self.
    if app.is_own_bundle {
        return base(Disposition::Excluded(ExcludeReason::OwnBundle));
    }
    // 2. Non-ordinary activation policies never tile (rift parity: only
    //    `Regular` apps enumerate; loginwindow-style carve-outs need live
    //    acquisition and stay a product decision).
    match app.activation_policy {
        ActivationPolicy::Accessory => {
            return base(Disposition::Excluded(ExcludeReason::AccessoryPolicy));
        }
        ActivationPolicy::Prohibited => {
            return base(Disposition::Excluded(ExcludeReason::ProhibitedPolicy));
        }
        ActivationPolicy::Regular | ActivationPolicy::Unknown => {}
    }
    // 3. Minimized windows and hidden-app windows leave layout (core stores
    //    the restore slot) and rejoin on return. Omission is not destruction.
    if ax.minimized {
        return base(Disposition::Excluded(ExcludeReason::Minimized));
    }
    if app.hidden {
        return base(Disposition::Excluded(ExcludeReason::HiddenApp));
    }
    // 4. Sheets float as normalized exception members (no parent
    //    resolution without live acquisition). A sheet reporting fullscreen
    //    keeps both facts: the float origin plus the overlay flag. Sheets
    //    keep their exception at any CG layer.
    if ax.role.as_deref() == Some(ROLE_SHEET) {
        return base(Disposition::Float(FloatReason::Sheet));
    }
    // 5. Dialog-family subroles float as normalized exception members,
    //    fullscreen or not and at any CG layer: the overlay flag rides
    //    alongside truthfully and the wire hold keys off it, so the float
    //    origin is never dropped.
    if let Some(subrole) = ax.subrole.as_deref()
        && FLOAT_SUBROLES.contains(&subrole)
    {
        return base(Disposition::Float(FloatReason::DialogSubrole));
    }
    // 6. Modal windows float (product R-SPC-01 transient rule), likewise
    //    orthogonal to the overlay fact and independent of CG layer.
    if ax.modal {
        return base(Disposition::Float(FloatReason::Modal));
    }
    // 7. Nonordinary CG layer with no recognized float exception: the
    //    shapes above already returned, so this is conservative exclusion
    //    pending an explicit TBD layer discriminator — never a claim about
    //    what lives at any layer.
    if cg.layer != 0 {
        return base(Disposition::Excluded(ExcludeReason::NonordinaryLayer));
    }
    // 8. Only standard windows tile. A present non-window role, or a
    //    missing/unknown/other subrole, excludes conservatively (yabai
    //    ignores `AXUnknown`; rift/AeroSpace filter nonstandard popups).
    //    A missing role stays admissible (rift parity for synthesized
    //    entries), but it still needs a standard subrole below. Unknown
    //    popups never hold, even fullscreen.
    if ax.role.as_deref().is_some_and(|role| role != ROLE_WINDOW) {
        return base(Disposition::Excluded(ExcludeReason::UnknownRole));
    }
    if ax.subrole.as_deref() != Some(SUBROLE_STANDARD) {
        return base(Disposition::Excluded(ExcludeReason::UnknownRole));
    }
    // 9. Native fullscreen without a float origin: the wire layer holds
    //    born arrivals slotless off the truthful overlay flag (shared gate)
    //    and keeps retained members' reserved slot with no writes.
    if ax.fullscreen {
        return base(Disposition::Fullscreen);
    }
    // 10. Ordinary tile candidate. Hints pass through for the core's
    //     admission-only fixed classifier; `size_settable` is carried, never
    //     consulted (TBD lane: no resizability inference).
    base(Disposition::Tile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> (AxFacts, CgFacts, AppFacts) {
        (AxFacts::standard(), CgFacts::normal(), AppFacts::regular())
    }

    fn classify(ax: &AxFacts, cg: &CgFacts, app: &AppFacts) -> ClassifiedWindow {
        classify_window(
            WindowFrame {
                wid: 7,
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            ax,
            cg,
            app,
        )
    }

    #[test]
    fn ordinary_standard_window_tiles_with_facts_carried() {
        let (ax, cg, app) = facts();
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Tile);
        assert_eq!(out.hints, WindowSizeHints::none());
        assert!(out.size_settable);
        let observed = out.to_observed().expect("tile submits");
        assert!(!observed.floating && !observed.fullscreen);
        assert_eq!((observed.wid, observed.w, observed.h), (7, 800, 600));
    }

    #[test]
    fn settable_only_lane_tiles_identically_either_way() {
        // TBD lane pin: `AXSize` settability alone never classifies. Both
        // shapes must render byte-identical observations while the carried
        // fact is preserved for the later decision.
        let (base_ax, cg, app) = facts();
        let mut settable = base_ax.clone();
        let mut unsettable = base_ax;
        unsettable.size_settable = false;
        settable.size_settable = true;
        let a = classify(&settable, &cg, &app);
        let b = classify(&unsettable, &cg, &app);
        assert_eq!(a.disposition, Disposition::Tile);
        assert_eq!(b.disposition, Disposition::Tile);
        assert_eq!(a.to_observed(), b.to_observed());
        assert!(a.size_settable);
        assert!(!b.size_settable);
    }

    #[test]
    fn unknown_hints_admit_normally() {
        // No inference from resizability and none from absent hints.
        let (mut ax, cg, app) = facts();
        ax.size_settable = false;
        ax.hints = WindowSizeHints::none();
        assert_eq!(classify(&ax, &cg, &app).disposition, Disposition::Tile);
    }

    #[test]
    fn dialog_family_floats_as_exception_members() {
        let (mut ax, cg, app) = facts();
        for subrole in [SUBROLE_DIALOG, SUBROLE_FLOATING, SUBROLE_SYSTEM_DIALOG] {
            ax.subrole = Some(subrole.to_owned());
            let out = classify(&ax, &cg, &app);
            assert_eq!(
                out.disposition,
                Disposition::Float(FloatReason::DialogSubrole),
                "{subrole}"
            );
            let observed = out.to_observed().expect("float submits");
            assert!(observed.floating && !observed.fullscreen);
        }
        // Sheets float by role even with a standard subrole.
        ax.role = Some(ROLE_SHEET.to_owned());
        ax.subrole = Some(SUBROLE_STANDARD.to_owned());
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Float(FloatReason::Sheet));
        assert!(out.to_observed().expect("sheet submits").floating);
    }

    #[test]
    fn modal_standard_window_floats() {
        let (mut ax, cg, app) = facts();
        ax.modal = true;
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Float(FloatReason::Modal));
    }

    #[test]
    fn fullscreen_without_float_origin_reaches_hold_gate() {
        // Plain fullscreen arrives unfloated with the overlay flag set; the
        // wire hold keys off the flag plus core-session lifetime.
        let (mut ax, cg, app) = facts();
        ax.fullscreen = true;
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Fullscreen);
        let observed = out.to_observed().expect("fullscreen submits");
        assert!(!observed.floating && observed.fullscreen);
    }

    #[test]
    fn fullscreen_dialog_keeps_its_float_origin() {
        // Float origins survive fullscreen: a dialog reporting fullscreen
        // stays a dialog exception with the overlay flag truthful, never
        // re-typed to a plain hold.
        let (mut ax, cg, app) = facts();
        ax.fullscreen = true;
        ax.subrole = Some(SUBROLE_DIALOG.to_owned());
        let out = classify(&ax, &cg, &app);
        assert_eq!(
            out.disposition,
            Disposition::Float(FloatReason::DialogSubrole)
        );
        let observed = out.to_observed().expect("dialog submits");
        assert!(observed.floating && observed.fullscreen);
        // Unknown popups never hold, even fullscreen.
        ax.subrole = Some(SUBROLE_UNKNOWN.to_owned());
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::UnknownRole)
        );
    }

    #[test]
    fn minimized_and_hidden_apps_exclude_without_close() {
        let (mut ax, cg, mut app) = facts();
        ax.minimized = true;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::Minimized)
        );
        assert!(classify(&ax, &cg, &app).to_observed().is_none());
        ax.minimized = false;
        app.hidden = true;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::HiddenApp)
        );
        assert!(classify(&ax, &cg, &app).to_observed().is_none());
    }

    #[test]
    fn off_space_onscreen_state_never_excludes() {
        // `onscreen=false` with a normal layer is an alive window on another
        // Space (or a hidden preimage): it must keep its facts, never read
        // as destroyed. Native Space homing stays later work.
        let (ax, mut cg, app) = facts();
        cg.onscreen = false;
        assert_eq!(classify(&ax, &cg, &app).disposition, Disposition::Tile);
        // A nonordinary layer with no recognized float exception excludes
        // instead, pending an explicit TBD layer discriminator.
        cg.layer = 25;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::NonordinaryLayer)
        );
    }

    #[test]
    fn nonzero_layer_float_exceptions_survive() {
        // Panels, system dialogs, sheets, and modals keep their exception
        // at nonzero native floating levels; the truthful overlay fact
        // rides along. Shapes with no recognized exception stay excluded.
        let (ax, mut cg, app) = facts();
        cg.layer = 3;
        let mut panel = ax.clone();
        panel.subrole = Some(SUBROLE_FLOATING.to_owned());
        let out = classify(&panel, &cg, &app);
        assert_eq!(
            out.disposition,
            Disposition::Float(FloatReason::DialogSubrole)
        );
        assert!(out.to_observed().expect("panel submits").floating);
        panel.fullscreen = true;
        let overlaid = classify(&panel, &cg, &app);
        assert_eq!(
            overlaid.disposition,
            Disposition::Float(FloatReason::DialogSubrole)
        );
        let observed = overlaid.to_observed().expect("panel submits");
        assert!(observed.floating && observed.fullscreen);

        let mut system = ax.clone();
        system.subrole = Some(SUBROLE_SYSTEM_DIALOG.to_owned());
        cg.layer = 7;
        assert_eq!(
            classify(&system, &cg, &app).disposition,
            Disposition::Float(FloatReason::DialogSubrole)
        );

        let mut sheet = ax.clone();
        sheet.role = Some(ROLE_SHEET.to_owned());
        cg.layer = 5;
        assert_eq!(
            classify(&sheet, &cg, &app).disposition,
            Disposition::Float(FloatReason::Sheet)
        );

        let mut modal = ax.clone();
        modal.modal = true;
        cg.layer = 3;
        assert_eq!(
            classify(&modal, &cg, &app).disposition,
            Disposition::Float(FloatReason::Modal)
        );

        // No recognized exception at a nonordinary layer: conservative
        // exclusion, with standard and unknown shapes alike.
        cg.layer = 3;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::NonordinaryLayer)
        );
        let mut unknown = ax.clone();
        unknown.subrole = Some(SUBROLE_UNKNOWN.to_owned());
        assert_eq!(
            classify(&unknown, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::NonordinaryLayer)
        );
    }

    #[test]
    fn unknown_roles_exclude_conservatively() {
        let (mut ax, cg, app) = facts();
        // Unknown subrole popups (tooltips, pills, keyboard switch) never tile.
        ax.subrole = Some(SUBROLE_UNKNOWN.to_owned());
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::UnknownRole)
        );
        // Missing subrole with a window role: still excluded (needs standard).
        ax.subrole = None;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::UnknownRole)
        );
        // Non-window roles exclude even with a standard subrole.
        ax.role = Some("AXTextField".to_owned());
        ax.subrole = Some(SUBROLE_STANDARD.to_owned());
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::UnknownRole)
        );
        // A missing role with a standard subrole stays admissible (rift
        // parity for synthesized entries).
        ax.role = None;
        assert_eq!(classify(&ax, &cg, &app).disposition, Disposition::Tile);
    }

    #[test]
    fn accessory_and_prohibited_policies_exclude() {
        let (ax, cg, mut app) = facts();
        app.activation_policy = ActivationPolicy::Accessory;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::AccessoryPolicy)
        );
        app.activation_policy = ActivationPolicy::Prohibited;
        assert_eq!(
            classify(&ax, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::ProhibitedPolicy)
        );
        // Unknown policy never excludes on its own: window facts govern.
        app.activation_policy = ActivationPolicy::Unknown;
        assert_eq!(classify(&ax, &cg, &app).disposition, Disposition::Tile);
    }

    #[test]
    fn own_bundle_excludes_first_and_ghostty_tiles() {
        let (ax, cg, mut app) = facts();
        // Own bundle wins over every other fact, including fullscreen.
        let mut ax_fs = ax.clone();
        ax_fs.fullscreen = true;
        app.is_own_bundle = true;
        assert_eq!(
            classify(&ax_fs, &cg, &app).disposition,
            Disposition::Excluded(ExcludeReason::OwnBundle)
        );
        // Ghostty (regular ordinary app, standard window) tiles: no product
        // exclusion exists; only live tests protect it.
        app.is_own_bundle = false;
        assert_eq!(classify(&ax, &cg, &app).disposition, Disposition::Tile);
    }

    #[test]
    fn fullscreen_fact_survives_on_every_admitted_shape() {
        // Independent overlay fact: a dialog/sheet reporting fullscreen
        // renders BOTH flags, and a plain fullscreen arrival renders the
        // overlay flag with no float. The wire hold may add synthetic
        // floating later, but the fact itself is never dropped here.
        let (mut ax, cg, app) = facts();
        ax.subrole = Some(SUBROLE_DIALOG.to_owned());
        ax.fullscreen = true;
        let dialog = classify(&ax, &cg, &app);
        let observed = dialog.to_observed().expect("dialog submits");
        assert!(observed.floating, "dialog stays an exception member");
        assert!(
            observed.fullscreen,
            "fullscreen fact survives on dialog exceptions"
        );
        ax.subrole = Some(SUBROLE_STANDARD.to_owned());
        let plain = classify(&ax, &cg, &app);
        assert_eq!(plain.disposition, Disposition::Fullscreen);
        let observed = plain.to_observed().expect("fullscreen submits");
        assert!(!observed.floating);
        assert!(observed.fullscreen);
    }

    #[test]
    fn hints_pass_through_untouched_for_core_admission() {
        // Admission-only classification lives in the core: the adapter never
        // pre-decides from hints, it only carries the current vector (D2/D5
        // freshness included).
        use tiler_core::size_hints::{FixedSizePredicate, is_fixed_size_with};
        let fixed = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        };
        let (mut ax, cg, app) = facts();
        ax.hints = fixed;
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Tile);
        assert_eq!(out.to_observed().expect("tile submits").hints, fixed);
        assert!(is_fixed_size_with(fixed, FixedSizePredicate::BothAxes));
        // G-05 partial-zero: not fixed under both-axes, fixed under either.
        let partial = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(0),
            max_w: Some(640),
            max_h: Some(0),
        };
        ax.hints = partial;
        let out = classify(&ax, &cg, &app);
        assert_eq!(out.disposition, Disposition::Tile);
        assert!(!is_fixed_size_with(partial, FixedSizePredicate::BothAxes));
        assert!(is_fixed_size_with(partial, FixedSizePredicate::EitherAxis));
    }
}
