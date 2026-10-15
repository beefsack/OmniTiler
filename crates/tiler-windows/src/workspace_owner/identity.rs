use super::*;

/// Resolve which output a digit chord acts on: the cached active output when
/// it still exists, else the foreground monitor, else the pointer monitor,
/// else output ordering. Never invents a key.
#[must_use]
pub fn output_context(
    spaces: &ManagedWorkspaces,
    cached: Option<&str>,
    foreground: Option<&str>,
    pointer: Option<&str>,
) -> Option<String> {
    if let Some(key) = cached
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = foreground
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = pointer
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    spaces.output_keys().into_iter().next()
}

/// Fixed-size admission observation flags for one Engine window
/// (R-SPC-04 D1-D8, G-05). Carried per observation from live adapter state,
/// never inferred in core and never retained across observations:
///
/// - `fullscreen`: live native fullscreen overlay (D5 first-exit routing and
///   brand-new fullscreen holds). Sticky/maximized overlays keep their
///   existing adapter lanes and stay false here.
/// - `fixed_auto`: adapter-asserted automatic fixed-float origin (D4
///   fixed max-births admitted slotless beneath native maximize). Never set
///   from an intentional floating flag or fixed hints alone.
/// - `fixed_suppress`: explicit user tile win for this live client (D3/D6/D7:
///   unfloats, workspace-enable re-adoption, and restart-hydrated tile
///   overrides). The Engine records the win transactionally; the adapter's
///   runtime set plus on-window marker carry it across domain releases and
///   restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedAdmissionFlags {
    pub fullscreen: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
}

/// Stamp fixed-size admission observation flags onto an assembled Engine
/// event's windows. Windows without an entry keep the builder defaults
/// (all false), so existing builder call sites and fixtures behave exactly
/// as before until the production handle path stamps live state. Pure:
/// no reads, no writes, no topology effects.
///
/// Sticky-dominance note: the production stamp never asserts `fixed_auto`
/// for a sticky member (core never marks automatic for a sticky
/// observation), and sticky/maximized Engine flags stay adapter-owned
/// (false) because `SessionCommand::Admit` refuses overlaid observations -
/// stamping them would refuse legitimate ToggleFloat unfloats (notably
/// sticky-off-to-tile). Sticky intent therefore always wins over automatic
/// origin without touching the ToggleFloat binding.
pub fn stamp_fixed_admission_flags(
    windows: &mut [EngineWindow],
    flags: &std::collections::HashMap<WindowId, FixedAdmissionFlags>,
) {
    for window in windows.iter_mut() {
        if let Some(entry) = flags.get(&window.window) {
            window.fullscreen = entry.fullscreen;
            window.fixed_auto = entry.fixed_auto;
            window.fixed_suppress = entry.fixed_suppress;
        }
    }
}

/// Same-HWND reuse set: known member keys sharing the fresh window's HWND
/// with a different full identity. The fresh enumeration read is
/// authoritative: a HWND hosts exactly one process, so any other key for the
/// number is a destroyed window whose number the OS recycled while visible.
/// Live hidden claims are excluded: they own their HWND through the ledger
/// and audit path (retire with identity-safe reveal), never this repair.
#[must_use]
pub fn reused_hwnd_stale(
    known: &[WindowKey],
    hidden_claimed: &BTreeSet<WindowKey>,
    fresh: &WindowKey,
) -> Vec<WindowKey> {
    known
        .iter()
        .filter(|k| k.hwnd == fresh.hwnd && *k != fresh && !hidden_claimed.contains(*k))
        .cloned()
        .collect()
}

/// Visible-membership lifetime decision: the live member tag must equal the
/// tag stored at admission. A same-process HWND reuse starts without our
/// window-lifetime property (absent) or with a stale tag, so it fails closed
/// exactly where HWND/PID/creation all still agree. An empty stored tag
/// never matches: untracked windows are never trusted, only stamped.
#[must_use]
pub fn visible_lifetime_ok(stored: &str, live: Option<&str>) -> bool {
    !stored.is_empty() && live.is_some_and(|tag| tag == stored)
}

/// Recycled-HWND guard: the live window matches the stored member only when
/// HWND, PID, and process creation all agree. A same-process HWND reuse keeps
/// all three, so this guard alone never sees it: the visible-membership
/// lifetime property must also agree (see [`visible_lifetime_ok`]).
#[must_use]
pub fn member_matches(
    stored: &WindowKey,
    live_hwnd: u64,
    live_pid: u32,
    live_creation: &str,
) -> bool {
    stored.hwnd == live_hwnd && stored.pid == live_pid && stored.creation.as_str() == live_creation
}

/// Portable domain value for one `(output, workspace)` pair.
#[must_use]
pub fn workspace_domain(
    output: &str,
    workspace: &str,
    bounds: Rect,
    gap: i32,
) -> (OutputDomain, DomainKey) {
    let key = DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds,
        gap,
        adjacent: BTreeMap::new(),
    };
    (domain, key)
}
