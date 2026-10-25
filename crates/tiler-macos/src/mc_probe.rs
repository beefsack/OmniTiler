#![forbid(unsafe_code)]

//! Mission Control probe policy: pure gates, validators, budgets, and report
//! shapes. No live dispatch here; [`crate::mc_lifecycle`] owns the run.
//!
//! Prior art (local checkouts only; pinned upstream URLs):
//! yabai toggles Mission Control with `space_manager_focus_space(sid)` plus
//! the private `CoreDockSendNotification(CFSTR("com.apple.expose.awake"), 0)`
//! (create/destroy are scripting-addition only; the non-SA focus fallback is
//! a synthesized Dock-swipe gesture, never implemented here):
//! https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L247-L251
//! Hammerspoon drives create/goto/remove with public AX on the Dock
//! (`mc` → `mc.display` on `AXDisplayID` → `mc.spaces` → `mc.spaces.add`
//! press, `mc.spaces.list[i]` press/remove-desktop; private
//! `_coreDesktopNotification("com.apple.expose.awake")` toggle; 0.3 s
//! stabilize; 2 s group poll; jank unavoidable):
//! https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L84-L152
//! Window/chrome classification precedent: yabai ignores the Finder desktop
//! AX element (empty window id) in window-equality checks:
//! https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1629
//! rift enumerates with `ExcludeDesktopElements` and prefers user spaces so
//! Desktop windows are not misidentified:
//! https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L470
//! https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L514
//! glide filters below-normal layers (desktop layer -1) and screensaver 25:
//! https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/window_server.rs#L502
//!
//! This probe keeps Hammerspoon's public AX shape but replaces the private
//! toggle with public `NSWorkspace.launchApplication("Mission Control")`:
//! app launch needs no Accessibility trust and no Automation consent. AX
//! legs still need observed (never prompted) trust. No synthetic input,
//! AppleScript, or System Events anywhere. TCC: launch needs no grant; AX
//! needs trust; `CGPreflightPostEventAccess` is observed context only.

use serde::{Deserialize, Serialize};

/// Exact Dock bundle: the only AX target. Exact match, never substring;
/// protected bundles refuse even on PID coincidence.
pub const DOCK_BUNDLE_ID: &str = "com.apple.dock";
/// Read-only Ghostty inclusion substring for the protective snapshot.
/// Deliberately broader than refusal matching: reads must never miss an
/// ordinary Ghostty window. Inclusion is read-only and never excludes.
pub const GHOSTTY_SUBSTR: &str = "ghostty";
/// Mission Control AX identifiers (Hammerspoon shape).
pub const MC_ID_ROOT: &str = "mc";
pub const MC_ID_DISPLAY: &str = "mc.display";
pub const MC_ID_SPACES: &str = "mc.spaces";
pub const MC_ID_SPACES_ADD: &str = "mc.spaces.add";
pub const MC_ID_SPACES_LIST: &str = "mc.spaces.list";
/// AX actions (press to create/go-to, remove-desktop to destroy).
pub const MC_ACTION_PRESS: &str = "AXPress";
pub const MC_ACTION_REMOVE_DESKTOP: &str = "AXRemoveDesktop";
/// MC-group poll bound (Hammerspoon 2 s parity) per open/close wait.
pub const MC_GROUP_POLL_MS: u64 = 2_000;
/// Post-toggle stabilize delay before traversal (Hammerspoon `MCwaitTime`).
pub const MC_STABILIZE_MS: u64 = 300;
/// Per-element AX messaging timeout (seconds) for every traversal leg.
pub const MC_AX_TIMEOUT_SECS: f32 = 2.0;
/// Bound per action including its readback.
pub const MC_ACTION_TIMEOUT_MS: u64 = 8_000;
/// Async-settle window for goto/remove/current-restore effect readback: a
/// sent press settles asynchronously (an observed goto took effect after the
/// immediate post-press snapshot), so the effect validators poll fresh
/// snapshots within this window after a failed immediate readback. Poll
/// only — a dispatched press is never replayed. Bounded by the existing
/// switch/readback plan (at most 3 seconds).
pub const MC_EFFECT_SETTLE_MS: u64 = 3_000;
/// Cleanup reserve (seconds) held back from the run budget so restoration
/// ability is never lost. Cleanup legs run inside the reserve.
pub const MC_CLEANUP_RESERVE_SECS: u64 = 15;
/// Minimum lifecycle `--seconds`: reserve plus minimum useful work. Smaller
/// budgets refuse upfront with a named reason instead of starting unrestorable.
pub const MC_MIN_LIFECYCLE_SECS: u64 = MC_CLEANUP_RESERVE_SECS + 30;
/// Hard dispatch cap (toggles, presses, restores). The time deadline is the
/// primary bound; this backstops pathological display counts.
pub const MC_MAX_ACTIONS: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McError {
    Unsupported(String),
    Ownership(String),
    Blocked(String),
    System(String),
    Dispatch(String),
}

impl std::fmt::Display for McError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::Ownership(r) => write!(f, "ownership refused: {r}"),
            Self::Blocked(r) => write!(f, "blocked: {r}"),
            Self::System(r) | Self::Dispatch(r) => write!(f, "mission-control probe error: {r}"),
        }
    }
}

impl std::error::Error for McError {}

/// Closed action vocabulary. No window variant (no close/quit/minimize/hide/
/// move/resize/Space-move of any window) and no synthetic-input variant: the
/// type system confines the lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McAction {
    Snapshot,
    Gate,
    AddSpace,
    GotoSpace,
    RemoveSpace,
    Cleanup,
}

impl McAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Gate => "gate",
            Self::AddSpace => "add-space",
            Self::GotoSpace => "goto-space",
            Self::RemoveSpace => "remove-space",
            Self::Cleanup => "cleanup",
        }
    }
}

/// One display's Space membership in SLS order. The in-row identifier
/// (managed-display UUID) stays internal, stripped from serialized reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySpaces {
    pub identifier: Option<String>,
    pub spaces: Vec<u64>,
    pub current: Option<u64>,
}

/// Fresh per-action topology: rebuilt from a new SLS read before every
/// action, never cached across one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologySnapshot {
    pub displays: Vec<DisplaySpaces>,
}

impl TopologySnapshot {
    /// Build from the exact-identifier SLS rows (never positional pairing).
    #[must_use]
    pub fn from_rows(rows: &[crate::ffi::SlsDisplayRow]) -> Self {
        Self {
            displays: rows
                .iter()
                .map(|r| DisplaySpaces {
                    identifier: r.identifier.clone(),
                    spaces: r.spaces.clone(),
                    current: r.current,
                })
                .collect(),
        }
    }

    /// Fresh SID join as (display, index). `None` is unknown/ambiguous.
    #[must_use]
    pub fn locate(&self, sid: u64) -> Option<(usize, usize)> {
        let mut found = None;
        for (di, display) in self.displays.iter().enumerate() {
            for (si, s) in display.spaces.iter().enumerate() {
                if *s == sid {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((di, si));
                }
            }
        }
        found
    }

    #[must_use]
    pub fn all_sids(&self) -> Vec<u64> {
        self.displays
            .iter()
            .flat_map(|d| d.spaces.iter().copied())
            .collect()
    }

    /// Sanitized flat layout for reports: sorted IDs plus currents, no UUIDs.
    #[must_use]
    pub fn sanitized(&self) -> McLayout {
        let mut space_ids = self.all_sids();
        space_ids.sort_unstable();
        space_ids.dedup();
        let mut current: Vec<u64> = self.displays.iter().filter_map(|d| d.current).collect();
        current.sort_unstable();
        current.dedup();
        McLayout {
            space_ids: space_ids.clone(),
            current,
            space_count: space_ids.len(),
        }
    }

    /// Ordered per-display layouts (row index pseudonymizes the display;
    /// identifiers stay internal). Reports carry these, never UUIDs.
    #[must_use]
    pub fn ordered_layouts(&self) -> Vec<McDisplayLayout> {
        self.displays
            .iter()
            .enumerate()
            .map(|(index, d)| McDisplayLayout {
                index,
                spaces: d.spaces.clone(),
                current: d.current,
            })
            .collect()
    }

    fn has_duplicates(&self) -> bool {
        let mut sids = self.all_sids();
        sids.sort_unstable();
        sids.windows(2).any(|w| w[0] == w[1])
    }
}

/// Shape gate over plain data: equal display counts plus exact per-display
/// identifier equality, before any positional zip. Identity first, order
/// never assumed.
pub fn check_same_shape(
    before: &TopologySnapshot,
    after: &TopologySnapshot,
    leg: &str,
) -> Result<(), McError> {
    if before.displays.len() != after.displays.len() {
        return Err(McError::Blocked(format!(
            "{leg} readback: display count changed ({} -> {}); refusing to claim anything",
            before.displays.len(),
            after.displays.len()
        )));
    }
    for (di, (b, a)) in before
        .displays
        .iter()
        .zip(after.displays.iter())
        .enumerate()
    {
        if b.identifier != a.identifier {
            return Err(McError::Blocked(format!(
                "{leg} readback: display {di} identity changed; refusing positional reasoning"
            )));
        }
    }
    Ok(())
}

/// Sanitized flat layout (IDs and currents only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McLayout {
    pub space_ids: Vec<u64>,
    pub current: Vec<u64>,
    pub space_count: usize,
}

/// Ordered per-display layout (index pseudonym; no identifiers on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McDisplayLayout {
    pub index: usize,
    pub spaces: Vec<u64>,
    pub current: Option<u64>,
}

/// Require observed Accessibility trust. `None`/`Some(false)` refuse as
/// consent shortfalls — never SIP verdicts, never prompted.
pub fn require_ax_trust(trusted: Option<bool>) -> Result<(), McError> {
    match trusted {
        Some(true) => Ok(()),
        other => Err(McError::Blocked(format!(
            "accessibility trust observed {other:?}; grant the probe bundle Accessibility consent via the LaunchServices route and retry — no consent UI is scripted here, and this is not a SIP verdict"
        ))),
    }
}

/// Require the actual unlocked-desktop verdict immediately before an action.
/// Anything but `Unlocked` fails closed; frontmost inference never suffices.
pub fn require_unlocked_session(session: &crate::ffi::SessionState) -> Result<(), McError> {
    if session.unlocked() {
        Ok(())
    } else {
        Err(McError::Blocked(format!(
            "window-server session is not unlocked (lock={:?}); refusing before any Mission Control touch — this is not a SIP verdict",
            session.lock
        )))
    }
}

/// Require the exact Dock bundle for the AX target PID. Empty, protected
/// (Ghostty/terminals included), or non-Dock bundles refuse before any AX
/// call — traversal never addresses a foreign app.
pub fn require_dock_bundle(bundle: &str) -> Result<(), McError> {
    if bundle.is_empty() {
        return Err(McError::Ownership(
            "refusing Mission Control traversal: target bundle unreadable".to_owned(),
        ));
    }
    if crate::ownership::is_protected_bundle(bundle) {
        return Err(McError::Ownership(format!(
            "refusing Mission Control traversal on protected bundle '{bundle}'"
        )));
    }
    if bundle != DOCK_BUNDLE_ID {
        return Err(McError::Ownership(format!(
            "refusing Mission Control traversal: target bundle '{bundle}' != '{DOCK_BUNDLE_ID}'"
        )));
    }
    Ok(())
}

/// Create-effect gate: same shape, no duplicate SIDs on either side, exactly
/// one new SID overall on the chosen display, preexisting order and currents
/// intact. Returns the created SID.
pub fn validate_create_effect(
    before: &TopologySnapshot,
    after: &TopologySnapshot,
    display: usize,
) -> Result<u64, McError> {
    check_same_shape(before, after, "create")?;
    if display >= after.displays.len() {
        return Err(McError::Blocked(format!(
            "create readback: display {display} out of range ({} displays)",
            after.displays.len()
        )));
    }
    if before.has_duplicates() || after.has_duplicates() {
        return Err(McError::Blocked(
            "create readback: duplicate Space IDs present; uniqueness unprovable — refusing to adopt a temp"
                .to_owned(),
        ));
    }
    let before_set: std::collections::HashSet<u64> = before.all_sids().into_iter().collect();
    let mut new: Vec<u64> = after
        .all_sids()
        .into_iter()
        .filter(|s| !before_set.contains(s))
        .collect();
    new.sort_unstable();
    new.dedup();
    if new.len() != 1 {
        return Err(McError::Blocked(format!(
            "create readback: expected exactly one new Space, observed {} new ({new:?}); refusing to adopt an ambiguous temp",
            new.len(),
        )));
    }
    let created = new[0];
    if !after.displays[display].spaces.contains(&created) {
        return Err(McError::Blocked(format!(
            "create readback: new Space {created} is not on chosen display {display}; refusing to adopt it"
        )));
    }
    for (di, (b, a)) in before
        .displays
        .iter()
        .zip(after.displays.iter())
        .enumerate()
    {
        let mut actual = a.spaces.clone();
        if di == display {
            if let Some(pos) = actual.iter().position(|s| *s == created) {
                actual.remove(pos);
            } else {
                return Err(McError::Blocked(format!(
                    "create readback: new Space {created} missing on display {di} at recheck"
                )));
            }
        }
        if actual != b.spaces {
            return Err(McError::Blocked(format!(
                "create readback: preexisting order changed on display {di} (before={:?} after-minus-new={actual:?})",
                b.spaces,
            )));
        }
        if a.current != b.current {
            return Err(McError::Blocked(format!(
                "create readback: current changed on display {di} (before={:?} after={:?}); create must not switch Spaces",
                b.current, a.current
            )));
        }
    }
    Ok(created)
}

/// Goto-effect gate: same shape, Space sets identical everywhere, target
/// display's current equals the target, every other current unchanged.
pub fn validate_goto_effect(
    before: &TopologySnapshot,
    after: &TopologySnapshot,
    display: usize,
    target: u64,
) -> Result<(), McError> {
    check_same_shape(before, after, "goto")?;
    if display >= after.displays.len() {
        return Err(McError::Blocked(format!(
            "goto readback: display {display} out of range"
        )));
    }
    for (di, (b, a)) in before
        .displays
        .iter()
        .zip(after.displays.iter())
        .enumerate()
    {
        if a.spaces != b.spaces {
            return Err(McError::Blocked(format!(
                "goto readback: Space set changed on display {di}; goto must not create or destroy Spaces"
            )));
        }
        let want = if di == display {
            Some(target)
        } else {
            b.current
        };
        if a.current != want {
            return Err(McError::Blocked(format!(
                "goto readback: current on display {di} is {:?}, want {want:?}",
                a.current,
            )));
        }
    }
    Ok(())
}

/// Removal-effect gate: same shape, created SID absent everywhere,
/// preexisting order intact, currents equal the pre-removal readback.
pub fn validate_removal_effect(
    before_remove: &TopologySnapshot,
    after: &TopologySnapshot,
    created: u64,
) -> Result<(), McError> {
    check_same_shape(before_remove, after, "removal")?;
    if after.all_sids().contains(&created) {
        return Err(McError::Blocked(format!(
            "removal readback: temp Space {created} still present; reporting residue, not success"
        )));
    }
    for (di, (b, a)) in before_remove
        .displays
        .iter()
        .zip(after.displays.iter())
        .enumerate()
    {
        let mut expected = b.spaces.clone();
        if let Some(pos) = expected.iter().position(|s| *s == created) {
            expected.remove(pos);
        }
        if a.spaces != expected {
            return Err(McError::Blocked(format!(
                "removal readback: display {di} order/content changed beyond the temp removal (want={expected:?} got={:?})",
                a.spaces,
            )));
        }
        if a.current != b.current {
            return Err(McError::Blocked(format!(
                "removal readback: current changed on display {di} (before={:?} after={:?})",
                b.current, a.current
            )));
        }
    }
    Ok(())
}

/// Final restoration gate: `after` must equal the original `before` exactly —
/// same shape, same ordered spaces per display, same currents. The temp's
/// absence is implied (it was never in `before`).
pub fn require_exact_restoration(
    before: &TopologySnapshot,
    after: &TopologySnapshot,
) -> Result<(), McError> {
    check_same_shape(before, after, "final")?;
    for (di, (b, a)) in before
        .displays
        .iter()
        .zip(after.displays.iter())
        .enumerate()
    {
        if a.spaces != b.spaces {
            return Err(McError::Blocked(format!(
                "final readback: display {di} Spaces differ from baseline (want={:?} got={:?})",
                b.spaces, a.spaces
            )));
        }
        if a.current != b.current {
            return Err(McError::Blocked(format!(
                "final readback: display {di} current differs from baseline (want={:?} got={:?})",
                b.current, a.current
            )));
        }
    }
    Ok(())
}

/// Removal-eligibility gate: candidate uniquely located, inactive (not
/// current anywhere), non-last on its display. Emptiness is verified live
/// ([`verify_space_empty_members`]); this gate covers identity and type.
pub fn removal_eligibility(snapshot: &TopologySnapshot, created: u64) -> Result<usize, McError> {
    let (display, _) = snapshot.locate(created).ok_or_else(|| {
        McError::Ownership(format!(
            "temp Space {created} is not uniquely located (absent or ambiguous); ownership uncertain — cleanup blocked"
        ))
    })?;
    for (di, d) in snapshot.displays.iter().enumerate() {
        if d.current == Some(created) {
            return Err(McError::Blocked(format!(
                "temp Space {created} is current on display {di}; move away first — an active Space is never removed"
            )));
        }
    }
    if snapshot.displays[display].spaces.len() < 2 {
        return Err(McError::Blocked(format!(
            "temp Space {created} is the last Space on display {display}; refusing (last-user-Space destroy refused upstream too)"
        )));
    }
    Ok(display)
}

/// Recovery-topology gate over plain data: the chosen display must read
/// exactly `[original, temp]` in SLS order (the uniquely created temp never
/// reordered, never moved across displays), both SIDs uniquely located, and
/// the display current must be readable as one of the two (the temp when
/// residue is current, the original when already restored). Anything else
/// is drift — never adopted, never forced.
pub fn validate_recover_topology(
    snapshot: &TopologySnapshot,
    display: usize,
    original: u64,
    temp: u64,
) -> Result<(), McError> {
    if original == temp {
        return Err(McError::Blocked(
            "recover topology: original equals temp; nothing to distinguish — refusing".to_owned(),
        ));
    }
    let row = snapshot.displays.get(display).ok_or_else(|| {
        McError::Blocked(format!(
            "recover topology: display {display} unknown ({} displays); refusing",
            snapshot.displays.len()
        ))
    })?;
    if row.spaces != vec![original, temp] {
        return Err(McError::Blocked(format!(
            "recover topology: display {display} Spaces are {:?}, want ordered [original={original}, temp={temp}]; refusing without the exact adopted shape",
            row.spaces,
        )));
    }
    if snapshot.locate(original) != Some((display, 0))
        || snapshot.locate(temp) != Some((display, 1))
    {
        return Err(McError::Ownership(
            "recover topology: original/temp not uniquely located on the expected display/index; ownership uncertain — refusing"
                .to_owned(),
        ));
    }
    match row.current {
        Some(c) if c == original || c == temp => Ok(()),
        other => Err(McError::Blocked(format!(
            "recover topology: display {display} current is {other:?}, want original={original} or temp={temp}; drifted — refusing"
        ))),
    }
}

/// Full-agreement Ghostty return over plain data: workspace frontmost and
/// SLPS front PID agree on one live PID whose bundle reads Ghostty, the
/// direct per-app leg is readable with its AX identity joining that PID,
/// and a nonzero selected window is present. Unknown on any leg is not a
/// return — never assumed from a bundle match alone. No focus writes
/// anywhere on this surface; the caller only reads.
#[must_use]
pub fn focus_ghostty_returned(after: &FocusLegs) -> bool {
    let Some(ws) = after.ws_pid else {
        return false;
    };
    if ws <= 1 || after.slps_pid != Some(ws) {
        return false;
    }
    let Some(bundle) = after.ws_bundle.as_deref() else {
        return false;
    };
    if !bundle.to_ascii_lowercase().contains(GHOSTTY_SUBSTR) {
        return false;
    }
    if !after.direct_readable {
        return false;
    }
    let Some(direct) = after.direct.as_ref() else {
        return false;
    };
    if direct.pid != ws || direct.ax_pid != Some(ws) {
        return false;
    }
    matches!(direct.selected_wid, Some(wid) if wid != 0)
}

/// One residual Space member under read-only inspection: identity plus
/// geometry plus membership. No titles, no content - the bundle names the
/// owner (e.g. the window-server accessibility chrome), the membership
/// names the blocking Space, and `exe` carries the already-collected owner
/// executable path (auditable compositor evidence, never inferred).
/// `ax_windows_absent` carries the optional owner AX-window inventory proof
/// (`Some(true)` only on a fresh readable empty inventory; absent in
/// reports captured before the proof existed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResidualMember {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub onscreen: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub spaces: Option<Vec<u64>>,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub exe_alias: Option<String>,
    #[serde(default)]
    pub ax_windows_absent: Option<bool>,
}

/// Bounded recovery report: restore-original-then-maybe-remove for one
/// previously adopted temp. `removed` is true only after the pre-existing
/// strict emptiness gate plus eligibility plus exact removal-effect
/// readback all pass; otherwise the temp parks with its residual members
/// enumerated and nothing declared clean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McRecoverReport {
    pub display: usize,
    pub temp_space: u64,
    pub original_space: u64,
    pub seconds: u64,
    pub layout_before: McLayout,
    pub displays_before: Vec<McDisplayLayout>,
    pub goto_layout: McLayout,
    pub layout_after: McLayout,
    pub displays_after: Vec<McDisplayLayout>,
    pub currents_restored: bool,
    pub separate_before: Option<bool>,
    pub separate_after: Option<bool>,
    pub mc_absent_at_end: bool,
    pub focus_before: FocusLegs,
    pub focus_after: FocusLegs,
    pub focus_ghostty_returned: bool,
    pub ghostty_before: Vec<GhosttyWindow>,
    pub ghostty_after: Vec<GhosttyWindow>,
    pub ghostty_equal: bool,
    pub residual_members: Vec<ResidualMember>,
    pub empty_checked: Option<u32>,
    pub empty_shell_skipped: Option<u32>,
    pub removal_blocked: Option<String>,
    pub removed: bool,
    pub latencies: Vec<LegLatency>,
    pub notes: Vec<String>,
    pub synthetic_events_posted: bool,
    pub caffeinate_reaped: bool,
}

/// Global run budget: one wall-clock deadline from `--seconds` with the
/// cleanup reserve held back, plus a dispatch counter backstop. Main-phase
/// dispatches spend only inside the work window (`deadline - reserve`);
/// cleanup legs spend inside the reserve up to the deadline.
pub struct ActionBudget {
    deadline: std::time::Instant,
    reserve: std::time::Duration,
    count: u32,
}

impl ActionBudget {
    #[must_use]
    pub fn new(seconds: u64) -> Self {
        Self {
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(seconds),
            reserve: std::time::Duration::from_secs(MC_CLEANUP_RESERVE_SECS),
            count: 0,
        }
    }

    pub fn spend_main(&mut self, what: &str) -> Result<(), McError> {
        if self.count >= MC_MAX_ACTIONS {
            return Err(McError::Blocked(format!(
                "budget exhausted: {what} refused past {MC_MAX_ACTIONS} dispatches"
            )));
        }
        if std::time::Instant::now() + self.reserve >= self.deadline {
            return Err(McError::Blocked(format!(
                "budget exhausted: {what} refused inside the cleanup reserve; restoring instead"
            )));
        }
        self.count += 1;
        Ok(())
    }

    pub fn spend_cleanup(&mut self, what: &str) -> Result<(), McError> {
        if self.count >= MC_MAX_ACTIONS {
            return Err(McError::Blocked(format!(
                "budget exhausted: cleanup {what} refused past {MC_MAX_ACTIONS} dispatches; residue reported"
            )));
        }
        if std::time::Instant::now() >= self.deadline {
            return Err(McError::Blocked(format!(
                "budget exhausted: cleanup {what} past the run deadline; residue reported"
            )));
        }
        self.count += 1;
        Ok(())
    }
}

/// Window class for the temp-emptiness scan. Shell chrome (Dock-owned
/// Mission Control/desktop chrome; below-normal desktop/wallpaper layers per
/// the glide layer seam; narrowly proven compositor-owned and sticky-system
/// windows below) skips the membership requirement and is counted.
/// Everything else — including every Ghostty window, never excluded — is a
/// candidate requiring a readable membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowClass {
    Shell,
    Candidate,
}

/// Pure shell classifier over plain data. Dock-exact plus sub-normal layers.
/// Ghostty is always a candidate — checked first, before the negative-layer
/// shell rule — so a Ghostty window is never misclassified as shell chrome.
/// Ordinary app bundles (Finder, terminals) are always candidates.
#[must_use]
pub fn classify_window(bundle: &str, layer: i32) -> WindowClass {
    if bundle.to_ascii_lowercase().contains(GHOSTTY_SUBSTR) {
        return WindowClass::Candidate;
    }
    if bundle == DOCK_BUNDLE_ID || layer < 0 {
        WindowClass::Shell
    } else {
        WindowClass::Candidate
    }
}

/// One enumerated window awaiting its membership verdict. `exe` is the
/// fresh executable-path read, normalized to [`WINDOWSERVER_EXE`] when a
/// versioned `proc_pidpath` alias canonical-matches (see
/// [`normalize_windowserver_alias`]; `None` when unreadable — never
/// inferred). `exe_alias` carries the raw alias read when normalization
/// applied (`None` otherwise), so the exclusion record retains the actual
/// observation. `onscreen`/`w`/`h` are the live geometry reads for the
/// sticky-chrome rule. `ax_windows_absent` is the optional owner AX-window
/// inventory proof for the ControlCenter rule only: `Some(true)` when a
/// fresh readable `AXWindows` read on the exact owner PID returned empty,
/// `Some(false)` when it returned a window, `None` when unreadable, timed
/// out, identity-mismatched, or never read (any non-shape window). `None`
/// never exempts. No titles anywhere on this surface.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowMember {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub onscreen: bool,
    pub w: f64,
    pub h: f64,
    pub exe: Option<String>,
    pub exe_alias: Option<String>,
    pub ax_windows_absent: Option<bool>,
}

/// Temp-emptiness verdict: counts plus one [`ShellExclusion`] per skipped
/// window. `checked` counts every candidate window whose membership was
/// read and did not contain the temp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptySummary {
    pub temp: u64,
    pub checked: u32,
    pub shell_skipped: u32,
    pub excluded: Vec<ShellExclusion>,
}

/// Exact canonical executable of the WindowServer compositor. The exemption
/// requires a fresh `process_executable_path` read equal to this string in
/// canonical form: `proc_pidpath` returns the versioned alias
/// (`.../SkyLight.framework/Versions/A/Resources/WindowServer`) while this
/// constant uses the `Current`-symlink form — both resolve to the same file,
/// so the match in [`classify_window_full`] compares canonical forms (see
/// [`normalize_windowserver_alias`]). Never an empty bundle, a PID guess,
/// or a layer heuristic.
pub const WINDOWSERVER_EXE: &str =
    "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer";

/// Canonical-form executable match for the WindowServer compositor: true
/// only when `raw` canonicalizes to exactly `canonical_const`. Names that
/// do not contain `WindowServer` refuse without touching the filesystem;
/// unreadable paths refuse (fail closed — never an exemption).
pub(crate) fn canonical_exe_equals(raw: &str, canonical_const: &std::path::Path) -> bool {
    if !raw.contains("WindowServer") {
        return false;
    }
    std::fs::canonicalize(raw)
        .ok()
        .is_some_and(|c| c.as_os_str() == canonical_const.as_os_str())
}

/// Normalize one fresh executable-path read for the compositor rule: exact
/// [`WINDOWSERVER_EXE`] passes through; a WindowServer-named alias that
/// canonical-matches normalizes to the constant; everything else (including
/// `None`) passes through untouched — never inferred, and `None` is never
/// claimed to mean a different owner. Returns `(exe_for_match,
/// raw_alias_when_normalized)`; the alias keeps the actual observation in
/// the exclusion record.
pub(crate) fn normalize_windowserver_alias(
    raw: Option<String>,
) -> (Option<String>, Option<String>) {
    let Some(exe) = raw else {
        return (None, None);
    };
    if exe == WINDOWSERVER_EXE {
        return (Some(exe), None);
    }
    let matched = std::fs::canonicalize(WINDOWSERVER_EXE)
        .ok()
        .is_some_and(|canon_const| canonical_exe_equals(&exe, &canon_const));
    if matched {
        (Some(WINDOWSERVER_EXE.to_owned()), Some(exe))
    } else {
        (Some(exe), None)
    }
}

/// Exact bundle of the window-server accessibility chrome. The legacy rule
/// requires offscreen, zero area, and every-user-Space membership; partial
/// attachment additionally requires the exact executable and layer below.
pub const A11Y_SERVER_BUNDLE: &str = "com.apple.AccessibilityUIServer";
/// Exact executable of that accessibility chrome, as read from the fresh
/// `process_executable_path` for its owner. Compared byte-exact; no alias
/// normalization is applied on this path.
pub const A11Y_SERVER_EXE: &str =
    "/System/Library/CoreServices/AccessibilityUIServer.app/Contents/MacOS/AccessibilityUIServer";

/// Exact bundle of the ControlCenter status-item chrome. The narrow rule
/// below exempts exactly one captured shape (offscreen layer 25, 120x30)
/// and only with a fresh readable empty owner AX window inventory.
pub const CONTROLCENTER_BUNDLE: &str = "com.apple.controlcenter";
/// Exact executable of that chrome, as read from the fresh
/// `process_executable_path` for its owner. Compared byte-exact; no alias
/// normalization is applied on this path.
pub const CONTROLCENTER_EXE: &str =
    "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter";
/// Exact CG layer of the exemptible ControlCenter shape (captured 120x30
/// status item). Ordinary layers never exempt.
pub const CONTROLCENTER_LAYER: i32 = 25;
/// Exact CG size of the exemptible ControlCenter shape.
pub const CONTROLCENTER_W: f64 = 120.0;
/// Exact CG size of the exemptible ControlCenter shape.
pub const CONTROLCENTER_H: f64 = 30.0;

/// Why one window was excluded as shell. Each variant names checkable
/// evidence carried on [`ShellExclusion`]; the three narrow rules below exist
/// because live metadata proved exactly these classes, and every nearby
/// variation outside their explicit predicates stays a candidate and blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellReason {
    /// Exact Dock bundle match.
    DockExact,
    /// Below-normal layer (desktop/wallpaper seam).
    SubNormalLayer,
    /// Fresh executable path exactly [`WINDOWSERVER_EXE`]: compositor-owned
    /// backing (e.g. per-Space menubar), regardless of bundle absence.
    WindowServerCompositor,
    /// Exact [`A11Y_SERVER_BUNDLE`] bundle, offscreen, zero area, and sticky
    /// over every enumerated user Space: window-server accessibility chrome.
    StickyAccessibilityChrome,
    /// Exact [`A11Y_SERVER_BUNDLE`] bundle plus exact [`A11Y_SERVER_EXE`],
    /// offscreen, layer 3, zero area, and membership containing the temp
    /// with every member SID inside the fresh user-Space set: the same
    /// accessibility chrome seen partially attached (target plus a strict
    /// user-Space subset). Unknown membership or any member SID outside the
    /// user set stays a candidate.
    SystemAccessibilityChrome,
    /// Exact [`CONTROLCENTER_BUNDLE`] bundle plus exact
    /// [`CONTROLCENTER_EXE`], offscreen, [`CONTROLCENTER_LAYER`] layer,
    /// 120x30, membership containing the temp with every member SID inside
    /// the fresh user-Space set, and a fresh readable empty owner AX window
    /// inventory (`ax_windows_absent == Some(true)`). Unknown/unreadable AX
    /// (`None`), nonempty AX (`Some(false)`), unknown membership, onscreen,
    /// wrong layer/geometry/owner, empty user set, temp absent, or any
    /// member SID outside the user set all stay candidates and block.
    ControlCenterChrome,
}

/// One shell exclusion with its identity evidence: which window, what
/// proved it system chrome, and a one-line detail for the log. Reports
/// carry these — never just counts — so the cleanup predicate is auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellExclusion {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub exe: Option<String>,
    pub reason: ShellReason,
    pub detail: String,
}

/// Pure exact-shape predicate for the ControlCenter exemption candidate:
/// exact [`CONTROLCENTER_BUNDLE`] bundle, exact [`CONTROLCENTER_EXE`]
/// executable, [`CONTROLCENTER_LAYER`] layer, offscreen, and exactly
/// 120x30. Shared by the live collector (which reads the AX proof only for
/// this shape) and the classifier below (which additionally requires the
/// proof plus target membership). Every nearby variation - normal-layer or
/// onscreen ControlCenter items, other geometries (including the captured
/// 94x30 items and the layer-2005 352x149 panel), other owners, unreadable
/// owners - fails here and stays a candidate. NaN dimensions never equal
/// the exact size, so nonfinite geometry fails closed.
#[must_use]
pub fn controlcenter_shape_matches(
    bundle: &str,
    exe: Option<&str>,
    layer: i32,
    onscreen: bool,
    w: f64,
    h: f64,
) -> bool {
    bundle == CONTROLCENTER_BUNDLE
        && exe == Some(CONTROLCENTER_EXE)
        && layer == CONTROLCENTER_LAYER
        && !onscreen
        && w == CONTROLCENTER_W
        && h == CONTROLCENTER_H
}

/// Pure narrow shell classifier over plain data: the base
/// ([`classify_window`]: Dock-exact, sub-normal layers) plus exactly three
/// evidence-based exemptions, in order. Ghostty is always a candidate
/// first — even with a compositor executable path or a sub-normal layer —
/// so an owned window is never exempted. There is deliberately NO generic
/// layer rule here: an ordinary window at a kept layer (glide keeps 0/3/8:
/// https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/window_server.rs#L277;
/// our observed chrome sits at layer 3, which that seam would keep) stays
/// a candidate unless its owner proves otherwise, and ordinary sticky
/// windows always block. Precedent for ignoring system chrome rather than
/// user windows: yabai ignores the Finder desktop AX element with an empty
/// window id (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1629);
/// rift enumerates with `ExcludeDesktopElements` and prefers user spaces so
/// Desktop windows are not misidentified
/// (https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/window_server.rs#L514).
/// `membership` is the window's SLS read (None when unreadable — never
/// assumed); `user_spaces` is the fresh topology user-SID set and must
/// contain `temp` (the live wrapper proves the temp user-typed before
/// calling). Returns the class plus the exclusion record when shell.
#[must_use]
pub fn classify_window_full(
    window: &WindowMember,
    membership: Option<&[u64]>,
    user_spaces: &[u64],
    temp: u64,
) -> (WindowClass, Option<ShellExclusion>) {
    let exclusion = |reason: ShellReason, detail: String| ShellExclusion {
        wid: window.wid,
        pid: window.pid,
        bundle: window.bundle.clone(),
        layer: window.layer,
        exe: window.exe.clone(),
        reason,
        detail,
    };
    // Ghostty first: owned windows are never shell chrome.
    if window.bundle.to_ascii_lowercase().contains(GHOSTTY_SUBSTR) {
        return (WindowClass::Candidate, None);
    }
    // Existing base rules, with the reason named.
    if window.bundle == DOCK_BUNDLE_ID {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::DockExact,
                format!("bundle == {DOCK_BUNDLE_ID}"),
            )),
        );
    }
    if window.layer < 0 {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::SubNormalLayer,
                format!("layer {} < 0 (desktop/wallpaper seam)", window.layer),
            )),
        );
    }
    // (1) WindowServer compositor: exact canonical executable path only.
    // Empty bundles, PID coincidences, and layers never exempt on their
    // own; an unreadable (`None`) executable never exempts either. When the
    // read was a versioned alias, the raw observation rides along in the
    // detail so the report never claims `None` meant a different owner.
    if window.exe.as_deref() == Some(WINDOWSERVER_EXE) {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::WindowServerCompositor,
                match &window.exe_alias {
                    Some(alias) => format!(
                        "exe == {WINDOWSERVER_EXE} (proc_pidpath alias {alias} canonicalized)"
                    ),
                    None => format!("exe == {WINDOWSERVER_EXE}"),
                },
            )),
        );
    }
    // (2) Sticky accessibility chrome: exact bundle AND offscreen AND zero
    // area AND sticky over every enumerated user Space (which includes the
    // temp by the caller's contract). Changed owner, nonzero frame,
    // onscreen, non-sticky, unknown membership, or an empty user set all
    // stay candidates and block below.
    if window.bundle == A11Y_SERVER_BUNDLE
        && !window.onscreen
        && window.w == 0.0
        && window.h == 0.0
        && !user_spaces.is_empty()
        && user_spaces.contains(&temp)
        && matches!(membership, Some(spaces) if user_spaces.iter().all(|s| spaces.contains(s)))
    {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::StickyAccessibilityChrome,
                format!(
                    "bundle == {A11Y_SERVER_BUNDLE}, offscreen, 0x0, sticky over {} user Spaces",
                    user_spaces.len(),
                ),
            )),
        );
    }
    // (3) System accessibility chrome, partial attachment: the same chrome
    // class as (2) with the exact executable pinned, offscreen, layer 3,
    // and zero area, but the membership need only contain the temp with
    // every member SID inside the fresh user set. Live CG+SLS showed this
    // chrome attached to a strict user-Space subset (present on the temp
    // and one other user Space, absent from a third), which (2) misses and
    // would block a proven-empty temp. Unknown owner/membership,
    // nonzero/NaN area, onscreen, wrong layer/exe/bundle, empty user set,
    // temp absent, or any member SID outside the user set all stay
    // candidates and block below.
    if window.bundle == A11Y_SERVER_BUNDLE
        && window.exe.as_deref() == Some(A11Y_SERVER_EXE)
        && !window.onscreen
        && window.layer == 3
        && window.w == 0.0
        && window.h == 0.0
        && !user_spaces.is_empty()
        && user_spaces.contains(&temp)
        && matches!(membership, Some(spaces) if spaces.contains(&temp) && spaces.iter().all(|s| user_spaces.contains(s)))
    {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::SystemAccessibilityChrome,
                format!(
                    "bundle == {A11Y_SERVER_BUNDLE}, exe == {A11Y_SERVER_EXE}, offscreen layer 3 0x0, member of temp {temp} within {} user Spaces",
                    user_spaces.len(),
                ),
            )),
        );
    }
    // (4) ControlCenter status-item chrome: the exact captured shape only
    // (exact bundle + exe, offscreen layer 25, 120x30) with target
    // membership wholly inside the fresh user-Space set, plus the optional
    // owner AX-window inventory proof (`Some(true)` = fresh readable empty
    // inventory for the exact owner PID). The inventory is scoped to the
    // exact owner, so foreign popups never enter it; truncation cannot fake
    // empty (a truncated list is nonempty). Every nearby variation stays a
    // candidate: normal-layer or onscreen ControlCenter items, other
    // geometries, other/unreadable owners, nonfinite area, unknown or
    // foreign membership - and the captured layer-2005 352x149 panel.
    if controlcenter_shape_matches(
        &window.bundle,
        window.exe.as_deref(),
        window.layer,
        window.onscreen,
        window.w,
        window.h,
    ) && window.ax_windows_absent == Some(true)
        && !user_spaces.is_empty()
        && user_spaces.contains(&temp)
        && matches!(membership, Some(spaces) if spaces.contains(&temp) && spaces.iter().all(|s| user_spaces.contains(s)))
    {
        return (
            WindowClass::Shell,
            Some(exclusion(
                ShellReason::ControlCenterChrome,
                format!(
                    "bundle == {CONTROLCENTER_BUNDLE}, exe == {CONTROLCENTER_EXE}, offscreen layer {CONTROLCENTER_LAYER} 120x30, member of temp {temp} within {} user Spaces, owner pid {} AX windows readable empty",
                    user_spaces.len(),
                    window.pid,
                ),
            )),
        );
    }
    (WindowClass::Candidate, None)
}

/// Pure temp-emptiness gate over enumerated (window, membership) pairs:
/// shell windows are skipped with one [`ShellExclusion`] each (identity
/// evidence, not just counts); a candidate containing the temp fails as
/// non-empty; a candidate with an unreadable membership fails as unknown —
/// never assumed empty. `user_spaces` is the fresh topology user-SID set
/// (temp included); ordinary sticky windows and every Ghostty window block
/// here when they contain the temp.
pub fn verify_space_empty_members(
    members: &[(WindowMember, Result<Vec<u64>, String>)],
    temp: u64,
    user_spaces: &[u64],
) -> Result<EmptySummary, McError> {
    let mut checked = 0u32;
    let mut shell_skipped = 0u32;
    let mut excluded = Vec::new();
    for (window, membership) in members {
        let spaces = membership.as_ref().ok().map(Vec::as_slice);
        let (class, reason) = classify_window_full(window, spaces, user_spaces, temp);
        if class == WindowClass::Shell {
            shell_skipped += 1;
            excluded.push(reason.expect("shell always carries its exclusion record"));
            continue;
        }
        match membership {
            Ok(spaces) if spaces.contains(&temp) => {
                return Err(McError::Blocked(format!(
                    "temp Space {temp} is not empty: window {} (pid {}) is a member; refusing removal",
                    window.wid, window.pid
                )));
            }
            Ok(_) => checked += 1,
            Err(cause) => {
                return Err(McError::Blocked(format!(
                    "temp Space {temp} emptiness unknown: window {} (pid {}) membership unreadable ({cause}); refusing removal",
                    window.wid, window.pid
                )));
            }
        }
    }
    Ok(EmptySummary {
        temp,
        checked,
        shell_skipped,
        excluded,
    })
}

/// Selected-oracle focus legs (read-only): NSWorkspace frontmost, SLPS front
/// PID, and the direct per-app focused-window read on their agreement.
/// Serialized for before/after comparison; IDs only, never titles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusLegs {
    pub ws_pid: Option<i32>,
    pub ws_bundle: Option<String>,
    pub slps_pid: Option<i32>,
    pub direct: Option<crate::ffi::DirectFocusRead>,
    pub direct_readable: bool,
}

/// Full-agreement restoration verdict over plain data: every leg readable on
/// both sides and pairwise equal. Unknown (`None`/unreadable) on any side is
/// unrestored — never assumed from a bundle match alone.
#[must_use]
pub fn focus_restored(before: &FocusLegs, after: &FocusLegs) -> bool {
    if !before.direct_readable || !after.direct_readable {
        return false;
    }
    let Some(ws) = before.ws_pid else {
        return false;
    };
    if after.ws_pid != Some(ws) {
        return false;
    }
    if before.slps_pid.is_none() || before.slps_pid != after.slps_pid {
        return false;
    }
    before.direct == after.direct && before.ws_bundle == after.ws_bundle && before.direct.is_some()
}

/// One Ghostty window under protection: identity plus geometry plus Space
/// membership (read-only snapshot; frames compared, never written). No
/// titles anywhere on this surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GhosttyWindow {
    pub wid: u32,
    pub pid: u32,
    pub layer: i32,
    pub onscreen: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub spaces: Option<Vec<u64>>,
}

/// One measured leg (name plus wall milliseconds for that named leg only —
/// never an end-to-end effect claim; readback settle polling is excluded
/// unless the leg name says so).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegLatency {
    pub leg: String,
    pub ms: u64,
}

/// Read-only inspection report. Never launches and posts nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McInspectReport {
    pub session: crate::ffi::SessionState,
    pub ax_trusted: Option<bool>,
    pub post_event_access: Option<bool>,
    pub dock_pid: Option<i32>,
    pub dock_bundle: Option<String>,
    pub mc_group_present: Option<bool>,
    pub presence_error: Option<String>,
    pub layout: Option<McLayout>,
    pub layout_error: Option<String>,
    pub synthetic_events_posted: bool,
    pub mutates: bool,
}

/// Protected-evidence bundle for error reports: Ghostty snapshots and
/// focus legs from both sides of a failed battery, so the mismatch is
/// never dropped. Each side is `None` only before its baseline exists.
pub type McFailEvidence = (
    Option<Vec<GhosttyWindow>>,
    Option<Vec<GhosttyWindow>>,
    Option<FocusLegs>,
    Option<FocusLegs>,
);

/// Structured lifecycle error: stage, cause, temp ownership, preimages, MC
/// presence, notes, and per-leg latencies — residue always actionable.
/// Carries the protected Ghostty snapshots and focus legs whenever the run
/// captured them (None only before baselines exist), so a failed battery
/// never drops the evidence that names the mismatch. IDs and geometry
/// only — never titles, content, or paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McErrorReport {
    pub stage: String,
    pub action: String,
    pub error: String,
    pub temp_space: Option<u64>,
    pub layout_before: Option<McLayout>,
    pub layout_after: Option<McLayout>,
    pub currents_restored: bool,
    pub mc_open_after: Option<bool>,
    pub caffeinate_reaped: bool,
    pub ghostty_before: Option<Vec<GhosttyWindow>>,
    pub ghostty_after: Option<Vec<GhosttyWindow>>,
    pub focus_before: Option<FocusLegs>,
    pub focus_after: Option<FocusLegs>,
    pub notes: Vec<String>,
    pub latencies: Vec<LegLatency>,
}

/// Successful lifecycle report. Ordered per-display layouts pin the topology
/// (row indices pseudonymize displays); focus restoration is the full oracle
/// agreement; Ghostty frames/membership must read equal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McLifecycleReport {
    pub display: usize,
    pub temp_space: u64,
    pub seconds: u64,
    pub layout_before: McLayout,
    pub displays_before: Vec<McDisplayLayout>,
    pub created_layout: McLayout,
    pub goto_layout: McLayout,
    pub back_layout: McLayout,
    pub layout_after: McLayout,
    pub displays_after: Vec<McDisplayLayout>,
    pub currents_restored: bool,
    pub separate_before: Option<bool>,
    pub separate_after: Option<bool>,
    pub mc_absent_at_end: bool,
    pub focus_before: FocusLegs,
    pub focus_after: FocusLegs,
    pub focus_restored: bool,
    pub ghostty_before: Vec<GhosttyWindow>,
    pub ghostty_after: Vec<GhosttyWindow>,
    pub ghostty_equal: bool,
    pub empty_scan: EmptySummary,
    pub latencies: Vec<LegLatency>,
    pub notes: Vec<String>,
    pub synthetic_events_posted: bool,
    pub caffeinate_reaped: bool,
}

pub fn error_json(report: &McErrorReport) -> (i32, String) {
    let body = serde_json::to_string(report).unwrap_or_else(|e| {
        format!(
            "{{\"stage\":\"{}\",\"error\":\"encode: {e}\"}}",
            report.stage
        )
    });
    (1, body)
}

/// Cross-Space Ghostty stability over plain data: the same window set with
/// equal PID/WID, layer, bounds, and membership compared as sets.
/// `onscreen` is deliberately excluded — visibility legitimately flips when
/// the CURRENT Space moves across the comparison, so exact equality is
/// unsatisfiable on a real switch for windows confined to one Space. Only
/// the recovery cross-Space leg uses this; the lifecycle B0→B2 battery
/// keeps exact equality (same-current restoration must read identical).
#[must_use]
pub fn ghostty_stable_across_switch(before: &[GhosttyWindow], after: &[GhosttyWindow]) -> bool {
    ghostty_mismatch_detail(before, after).is_none()
}

/// Pure Ghostty mismatch detail over plain data (no OS touch): `None` when
/// [`ghostty_stable_across_switch`] holds, else the first cause (count,
/// identity, layer, bounds, or membership — naming unreadable membership as
/// unknown, never as equal). `onscreen` is excluded by design (see above),
/// so a visibility-only flip never reports a mismatch. The live runner logs
/// this when the stability gate reads false, distinguishing a real geometry
/// or membership change from an unreadable-membership artifact without
/// weakening the exact gate.
#[must_use]
pub fn ghostty_mismatch_detail(
    before: &[GhosttyWindow],
    after: &[GhosttyWindow],
) -> Option<String> {
    fn norm(spaces: &Option<Vec<u64>>) -> Option<Vec<u64>> {
        spaces.as_ref().map(|s| {
            let mut v = s.clone();
            v.sort_unstable();
            v.dedup();
            v
        })
    }
    if before.len() != after.len() {
        return Some(format!(
            "ghostty count {} -> {} (windows appeared/disappeared; onscreen excluded)",
            before.len(),
            after.len()
        ));
    }
    let mut a: Vec<&GhosttyWindow> = before.iter().collect();
    let mut b: Vec<&GhosttyWindow> = after.iter().collect();
    a.sort_by_key(|g| g.wid);
    b.sort_by_key(|g| g.wid);
    for (x, y) in a.iter().zip(b.iter()) {
        if x.wid != y.wid || x.pid != y.pid {
            return Some(format!(
                "ghostty identity WID{}/PID{} -> WID{}/PID{}",
                x.wid, x.pid, y.wid, y.pid
            ));
        }
        if x.layer != y.layer {
            return Some(format!(
                "ghostty WID{} layer {} -> {}",
                x.wid, x.layer, y.layer
            ));
        }
        if x.x != y.x || x.y != y.y || x.w != y.w || x.h != y.h {
            return Some(format!(
                "ghostty WID{} bounds {}x{}@{},{} -> {}x{}@{},{}",
                x.wid, x.w, x.h, x.x, x.y, y.w, y.h, y.x, y.y
            ));
        }
        if norm(&x.spaces) != norm(&y.spaces) {
            if x.spaces.is_none() || y.spaces.is_none() {
                return Some(format!(
                    "ghostty WID{} membership unreadable on one side ({:?} -> {:?}); unknown, never equal",
                    x.wid, x.spaces, y.spaces
                ));
            }
            return Some(format!(
                "ghostty WID{} membership {:?} -> {:?}",
                x.wid, x.spaces, y.spaces
            ));
        }
    }
    None
}

/// One Space's join in the live topology: display/index position, that
/// display's current, whether this Space is current, and its SLS type name.
/// Read-only location evidence for the cleanup predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McSpaceLocation {
    pub display: usize,
    pub index: usize,
    pub display_current: Option<u64>,
    pub is_current: bool,
    pub space_type: Option<String>,
}

/// Read-only Space-members report: every live window whose membership
/// contains the Space — shell chrome included — with identity, geometry,
/// and membership. Plus the read-only focus-oracle legs and the protected
/// Ghostty snapshot for the same instant, so one call carries the complete
/// bounded metadata the cleanup predicate needs. IDs, geometry, and the
/// already-collected owner executable path only - never titles or content.
/// No launch, no writes, no posts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McMembersReport {
    pub space: u64,
    pub session: crate::ffi::SessionState,
    pub ax_trusted: Option<bool>,
    pub layout: Option<McLayout>,
    pub layout_error: Option<String>,
    pub location: Option<McSpaceLocation>,
    pub members: Vec<ResidualMember>,
    pub focus: FocusLegs,
    pub ghostty: Vec<GhosttyWindow>,
    pub ghostty_error: Option<String>,
    pub synthetic_events_posted: bool,
    pub mutates: bool,
}
