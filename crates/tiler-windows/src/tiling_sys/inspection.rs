#![forbid(unsafe_code)]
use super::*;

/// Enumerate direct child windows of one parent. `None` on API failure.
fn enumerate_child_windows(parent: HWND) -> Option<Vec<isize>> {
    crate::ffi::enumerate_child_windows(parent as usize as u64).map(|children| {
        children
            .into_iter()
            .map(|hwnd| hwnd as usize as isize)
            .collect()
    })
}

/// Live executable paths of a top-level window's hosted children: direct
/// children owned by a different process than the top-level window itself.
/// Unreadable children contribute nothing (never a match). Scope
/// classification only: no effects touch child windows. Empty on enumeration
/// failure, which fails closed for listed hosts at the caller's gate.
fn hosted_child_exes(parent: HWND, top_pid: u32) -> Vec<String> {
    let mut out = Vec::new();
    let Some(children) = enumerate_child_windows(parent) else {
        return out;
    };
    for raw in children {
        let pid = crate::ffi::window_pid(raw as usize as u64);
        if pid == 0 || pid == top_pid {
            continue;
        }
        let exe = HeldProcess::open(pid)
            .ok()
            .and_then(|held| held.identity().ok())
            .filter(|ident| ident.pid == pid)
            .map(|ident| ident.exe_path);
        if let Some(exe) = exe {
            out.push(exe);
        }
    }
    out
}

/// Fresh hosted-child gate for one top-level window: listed hosts pass only
/// while a live hosted child matches. Short-circuits before enumerating when
/// the top-level executable names no pair. Read-only; the caller selects the
/// refusal outcome.
pub(super) fn hosted_gate_allows(
    top_exe: &str,
    hwnd_u64: u64,
    top_pid: u32,
    pairs: &[ScopeHostChild],
) -> bool {
    if pairs.is_empty() {
        return true;
    }
    if !pairs
        .iter()
        .any(|pair| scope_exe_basename(&pair.host) == scope_exe_basename(top_exe))
    {
        return true;
    }
    let hwnd = hwnd_u64 as isize as HWND;
    hosted_child_allows(top_exe, &hosted_child_exes(hwnd, top_pid), pairs)
}

fn exe_basename(exe_path: &str) -> String {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("unknown")
        .to_owned()
}

/// `children` command: read-only child-window report for explicitly listed
/// top-level targets. Each child carries HWND, pid, executable basename,
/// class, and full process identity when readable; never titles. Used to
/// bind hosted app windows (e.g. Calculator under an ApplicationFrameHost
/// root) to their owning process before any capture or mutation.
pub fn cmd_children(options: &ChildrenOptions) -> Result<String> {
    ensure_pm_v2()?;
    let mut windows = Vec::new();
    for hwnd_u64 in &options.hwnds {
        let parent = *hwnd_u64 as isize as HWND;
        let Some(raw_children) = enumerate_child_windows(parent) else {
            return Err(err("error: child enumeration failed"));
        };
        let mut children = Vec::new();
        for raw in raw_children {
            let hwnd = raw as HWND;
            let pid = crate::ffi::window_pid(raw as usize as u64);
            let identity = (pid != 0)
                .then(|| HeldProcess::open(pid).ok())
                .flatten()
                .and_then(|held| held.identity().ok())
                .filter(|ident| ident.pid == pid);
            let (exe, detail) = match identity {
                Some(ident) => (
                    exe_basename(&ident.exe_path),
                    serde_json::json!({
                        "pid": ident.pid,
                        "process_creation": ident.process_creation,
                        "exe_path": ident.exe_path,
                        "user_sid": ident.user_sid,
                        "session_id": ident.session_id,
                    }),
                ),
                None => ("unknown".to_owned(), serde_json::Value::Null),
            };
            children.push(serde_json::json!({
                "hwnd": raw as u64,
                "pid": pid,
                "exe": exe,
                "class": class_of(hwnd),
                "identity": detail,
            }));
        }
        windows.push(serde_json::json!({ "hwnd": *hwnd_u64, "children": children }));
    }
    Ok(serde_json::json!({ "windows": windows }).to_string())
}

fn rect_array(rect: &Rect) -> [i32; 4] {
    [rect.x, rect.y, rect.w, rect.h]
}

pub(super) fn medium_caller() -> Result<ProcessIdentity> {
    let me = crate::native::current_identity().map_err(|e| err(format!("error: identity {e}")))?;
    let rid = crate::native::current_integrity_level()
        .map_err(|e| err(format!("error: integrity {e}")))?;
    if !is_medium_rid(rid) {
        return Err(err(format!("refuse: integrity {rid} is not medium")));
    }
    Ok(me)
}

fn read_allowlist(path: &Path) -> Result<Vec<AllowEntry>> {
    let text =
        std::fs::read_to_string(path).map_err(|e| err(format!("error: allowlist read: {e}")))?;
    parse_allowlist(&text).map_err(err)
}

/// One inspection entry without frame geometry: full frozen-matchable
/// identity, an accurate skip, and no fabricated rectangles. `outer` and
/// `visible` are null; `dpi` and `minimized` are real queried state.
fn noframe_inspect_report(
    entry: &AllowEntry,
    identity: &ObservedTarget,
    skip: SkipReason,
    minimized: bool,
    hwnd: HWND,
) -> serde_json::Value {
    let dpi = crate::ffi::dpi_for_window(hwnd as usize as u64);
    serde_json::json!({
        "hwnd": entry.hwnd,
        "pid": identity.pid,
        "process_creation": identity.process_creation,
        "exe_path": identity.exe_path,
        "user_sid": identity.user_sid,
        "session_id": identity.session_id,
        "tag": identity.tag,
        "identity_match": true,
        "outer": null,
        "visible": null,
        "dpi": dpi,
        "minimized": minimized,
        "eligible": false,
        "skip": skip.as_str(),
    })
}

/// `inspect` command: read-only fresh-state report for exactly the frozen
/// allowlist. Identity and state resolve before any frame query, so a
/// legitimately hidden (passive) or minimized exact helper keeps full
/// identity match with an accurate skip and no fabricated rectangles; only a
/// genuinely unresolvable or non-matching identity reports
/// `identity-changed`. Raw native identity, fresh outer/visible rectangles
/// from one coherent observe pair when frames exist, per-window DPI, native
/// facts with proof-mode eligibility, and the current foreground HWND. No
/// titles, no writes.
pub fn cmd_inspect(options: &InspectOptions) -> Result<String> {
    use ObserveFailure::Known;
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let areas = all_monitors()?;
    let fulls = monitor_fulls(&areas);
    let allow = read_allowlist(&options.allowlist)?;
    let mut tokens = TokenMap::default();
    let foreground = crate::ffi::foreground_window();
    let mut windows = Vec::new();
    for entry in &allow {
        let hwnd = entry.hwnd as isize as HWND;
        // Stateless identity/state first, independent of geometry.
        let ident = window_identity(hwnd, entry.hwnd, &me);
        let matched = ident.as_ref().is_some_and(|i| allow_match(entry, i));
        let visible = crate::ffi::is_visible(entry.hwnd);
        let minimized = crate::ffi::is_iconic(entry.hwnd);
        match inspect_stateless_verdict(matched, visible, minimized) {
            StatelessVerdict::Report {
                identity_match,
                // No-frame reports are never eligible; the verdict type
                // guarantees this today.
                eligible: _,
                skip,
            } => {
                match (ident.as_ref(), identity_match) {
                    (Some(identity), true) => windows.push(noframe_inspect_report(
                        entry, identity, skip, minimized, hwnd,
                    )),
                    _ => windows.push(serde_json::json!({
                        "hwnd": entry.hwnd,
                        "identity_match": false,
                        "eligible": false,
                        "skip": skip.as_str(),
                    })),
                }
                continue;
            }
            StatelessVerdict::NeedsGeometry => {}
        }
        // Identity matched a visible, non-minimized window: frame queries and
        // `classify` still decide. A racy frame failure still reports the
        // known identity with its accurate reason, never `identity-changed`.
        match observe_window(hwnd, &me, &fulls, &mut tokens) {
            Ok(window) if allow_match(entry, &window.identity) => {
                let outer = window.outer;
                let dpi = crate::ffi::dpi_for_window(window.hwnd);
                let (eligible, skip) = match classify(&window.facts) {
                    Ok(()) => (true, None),
                    Err(reason) => (false, Some(reason.as_str().to_owned())),
                };
                windows.push(serde_json::json!({
                    "hwnd": window.hwnd,
                    "pid": window.identity.pid,
                    "process_creation": window.identity.process_creation,
                    "exe_path": window.identity.exe_path,
                    "user_sid": window.identity.user_sid,
                    "session_id": window.identity.session_id,
                    "tag": window.identity.tag,
                    "identity_match": true,
                    "outer": rect_array(&outer),
                    "visible": rect_array(&window.visible),
                    "dpi": dpi,
                    "facts": {
                        "minimized": window.facts.minimized,
                        "maximized": window.facts.maximized,
                        "cloaked": window.facts.cloaked,
                        "elevated": window.facts.elevated,
                        "shell": window.facts.shell,
                        "tool_window": window.facts.tool_window,
                        "owned": window.facts.owned,
                        "captionless_fullscreen": window.facts.captionless_fullscreen,
                        "no_activate": window.facts.no_activate,
                        "dialog": window.facts.dialog,
                    },
                    "eligible": eligible,
                    "skip": skip,
                }));
            }
            Err(Known(known, reason)) if allow_match(entry, &known.identity) => {
                windows.push(noframe_inspect_report(
                    entry,
                    &known.identity,
                    reason,
                    reason == SkipReason::Minimized,
                    hwnd,
                ));
            }
            _ => windows.push(serde_json::json!({
                "hwnd": entry.hwnd,
                "identity_match": false,
                "eligible": false,
                "skip": SkipReason::IdentityChanged.as_str(),
            })),
        }
    }
    Ok(serde_json::json!({ "windows": windows, "foreground": foreground }).to_string())
}
/// Read-only z-order relation for `border-inspect`: the owned overlay versus
/// the current foreground window in `EnumWindows` top-to-bottom order.
/// `(above, adjacent_below)`. A background owner cannot place its surface
/// above the foreground window, so adjacent-below (directly beneath the
/// target, ring uncovered) is the achievable correct placement. `(None, None)`
/// when either endpoint is gone. No writes.
fn overlay_z_relation(overlay_u64: u64) -> (Option<bool>, Option<bool>) {
    let foreground = crate::ffi::foreground_window();
    if overlay_u64 == 0 || foreground == 0 || overlay_u64 == foreground {
        return (None, None);
    }
    if !crate::ffi::is_window(foreground) || !crate::ffi::is_window(overlay_u64) {
        return (None, None);
    }
    let order = {
        let Some(order) = crate::ffi::enumerate_windows() else {
            return (None, None);
        };
        order
    };
    let mut upper_idx: Option<usize> = None;
    let mut lower_idx: Option<usize> = None;
    for (i, hwnd) in order.iter().enumerate() {
        if *hwnd == overlay_u64 && upper_idx.is_none() {
            upper_idx = Some(i);
        }
        if *hwnd == foreground && lower_idx.is_none() {
            lower_idx = Some(i);
        }
        if upper_idx.is_some() && lower_idx.is_some() {
            break;
        }
    }
    match (upper_idx, lower_idx) {
        (Some(u), Some(l)) if u < l => (Some(true), Some(false)),
        (Some(u), Some(l)) if u == l + 1 => (Some(false), Some(true)),
        (Some(_), Some(_)) => (Some(false), Some(false)),
        _ => (None, None),
    }
}
/// `border-inspect` command: read-only report of the running owner's
/// process-owned overlay window (geometry/visibility only). Binds the exact
/// ledger owner (creation/pid/exe/sid/session) and reports only that owner's
/// overlay class windows: `present` is false when the owner runs with
/// `--no-active-border` or has no eligible target. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `active-border`
/// log events instead.
pub fn cmd_border_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::OVERLAY_CLASS)
}

/// `underlay-inspect` command: read-only report of the running owner's
/// process-owned group-underlay fill (geometry/visibility only), mirroring
/// `border-inspect`. `present` means a carrier HWND was enumerated for this
/// owner (hidden HWNDs persist: `hide` keeps the window alive for the next
/// group, so `present` stays true with `visible=false` after the first show).
/// It is false when the owner runs with `--no-group-underlay` or before the
/// first show. No titles, no content, no screen capture; the owned-pixel
/// checksum rides the owner's `group-underlay` log events instead.
pub fn cmd_underlay_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::UNDERLAY_CLASS)
}

/// `preview-inspect` command: read-only report of the running owner's
/// process-owned drop-preview fill (geometry/visibility only), mirroring
/// `border-inspect`/`underlay-inspect`. `present` means a carrier HWND was
/// enumerated for this owner (hidden HWNDs persist after the first show).
/// It is false before the first preview sample. No titles, no content, no
/// screen capture; the owned-pixel checksum rides the owner's `drag-preview`
/// log events instead.
pub fn cmd_preview_inspect() -> Result<String> {
    inspect_carrier(crate::active_border_sys::PREVIEW_CLASS)
}

/// Shared read-only carrier inspection for one owned overlay class.
fn inspect_carrier(class: &str) -> Result<String> {
    ensure_pm_v2()?;
    let me = medium_caller()?;
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let ledger_text =
        std::fs::read_to_string(dir.join(crate::storage::LEDGER_FILE_NAME)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                err("refuse: no owner running")
            } else {
                err(format!("error: ledger read: {e}"))
            }
        })?;
    let record: crate::model::RecoveryLedger =
        crate::model::parse_ledger(&ledger_text).map_err(|_| err("refuse: corrupt ledger"))?;
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Err(err("refuse: owner mismatch"));
    }
    let exe = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    if !exe_paths_equal(&exe, &record.owner.exe_path) {
        return Err(err("refuse: owner mismatch"));
    }
    let held = HeldProcess::open(record.owner.pid).map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    let live = held.identity().map_err(|e| match e {
        crate::native::IdentityError::Absent => err("refuse: owner not running"),
        other => err(format!("error: owner {other}")),
    })?;
    if live != record.owner || !held.is_alive() {
        return Err(err("refuse: owner not running"));
    }
    let Some(hwnds) = enumerate_hwnds() else {
        return Err(err("error: enumeration failed"));
    };
    let mut overlays = Vec::new();
    for raw in hwnds {
        let hwnd = raw as HWND;
        if class_of(hwnd) != class {
            continue;
        }
        let pid = crate::ffi::window_pid(raw as usize as u64);
        if pid == 0 || pid != record.owner.pid {
            continue;
        }
        let Some(rect) = crate::ffi::window_rect(raw as usize as u64).and_then(rect_from_win)
        else {
            continue;
        };
        let exstyle = crate::ffi::window_exstyle(raw as usize as u64);
        let (above_fg, adjacent_fg) = overlay_z_relation(raw as usize as u64);
        overlays.push(serde_json::json!({
            "hwnd": raw as u64,
            "visible": crate::ffi::is_visible(raw as usize as u64),
            "rect": rect_array(&rect),
            "dpi": crate::ffi::dpi_for_window(raw as usize as u64),
            "exstyle": format!("{exstyle:08x}"),
            "topmost": exstyle & WS_EX_TOPMOST != 0,
            "above_foreground": above_fg,
            "adjacent_below_foreground": adjacent_fg,
        }));
    }
    let foreground = crate::ffi::foreground_window();
    Ok(serde_json::json!({
        "present": !overlays.is_empty(),
        "overlays": overlays,
        "foreground": foreground,
    })
    .to_string())
}
