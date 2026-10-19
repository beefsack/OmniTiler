#![forbid(unsafe_code)]
use serde::Serialize;

use crate::model::ProcessIdentity;

pub const TEST_WINDOW_CLASS: &str = "OmniTilerOwnedTest";
pub const TEST_WINDOW_PROP: &str = "OmniTilerLifetime";
pub const TEST_WINDOW_X: i32 = 200;
pub const TEST_WINDOW_Y: i32 = 200;
pub const TEST_WINDOW_W: i32 = 640;
pub const TEST_WINDOW_H: i32 = 480;
pub const HELPER_DEFAULT_SECONDS: u64 = 180;
pub const HELPER_MAX_SECONDS: u64 = 600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WindowSnapshot {
    pub hwnd: u64,
    pub process: ProcessIdentity,
    pub tag: String,
    pub visible: bool,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    Absent(String),
    Failed(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent(msg) => write!(f, "refuse: {msg}"),
            Self::Failed(msg) => write!(f, "error: {msg}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

#[must_use]
pub fn parse_hwnd(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok().filter(|v| *v != 0)
    } else {
        t.parse::<u64>().ok().filter(|v| *v != 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperRunOptions {
    pub receipt: std::path::PathBuf,
    pub seconds: u64,
    /// Opt-in passive creation: no `WS_VISIBLE`, no `ShowWindow`, no focus.
    /// The window stays hidden until an exact-bound `show` admission. Default
    /// (`false`) preserves the spike behavior.
    pub passive: bool,
}

pub fn parse_helper_run_args(args: &[String]) -> Result<HelperRunOptions, String> {
    let usage = "usage: run --receipt PATH [--seconds N] [--passive]";
    let mut receipt: Option<std::path::PathBuf> = None;
    let mut seconds = HELPER_DEFAULT_SECONDS;
    let mut passive = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--receipt" => {
                i += 1;
                let v = args.get(i).ok_or(usage)?;
                receipt = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let v = args.get(i).ok_or(usage)?;
                seconds = v.parse::<u64>().map_err(|_| usage.to_owned())?;
                i += 1;
            }
            "--passive" => {
                passive = true;
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let receipt = receipt.ok_or(usage)?;
    if seconds == 0 || seconds > HELPER_MAX_SECONDS {
        return Err(format!("refuse: seconds must be 1..={HELPER_MAX_SECONDS}"));
    }
    Ok(HelperRunOptions {
        receipt,
        seconds,
        passive,
    })
}

/// Exact-bound window reference for the `show`, `minimize`, and `restore`
/// helper commands: HWND plus the opaque lifetime tag from the run receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedHwnd {
    pub hwnd: u64,
    pub tag: String,
}

/// Parse `HWND --tag TAG` trailing arguments for one helper command.
pub fn parse_tagged_hwnd_args(command: &str, args: &[String]) -> Result<TaggedHwnd, String> {
    let usage = format!("usage: tiler-test-window {command} HWND --tag TAG");
    if args.len() != 3 || args[1] != "--tag" || args[2].is_empty() {
        return Err(usage);
    }
    let hwnd = parse_hwnd(&args[0]).ok_or(usage)?;
    Ok(TaggedHwnd {
        hwnd,
        tag: args[2].clone(),
    })
}

/// Exact-bound move target: `move HWND --tag TAG --to X,Y,W,H`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTarget {
    pub hwnd: u64,
    pub tag: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Parse `move HWND --tag TAG --to X,Y,W,H` (physical pixels, no activation).
pub fn parse_move_args(args: &[String]) -> Result<MoveTarget, String> {
    let usage = "usage: tiler-test-window move HWND --tag TAG --to X,Y,W,H".to_owned();
    if args.len() != 5 || args[1] != "--tag" || args[3] != "--to" {
        return Err(usage);
    }
    let hwnd = parse_hwnd(&args[0]).ok_or_else(|| usage.clone())?;
    if args[2].is_empty() {
        return Err(usage);
    }
    let parts: Vec<&str> = args[4].split(',').collect();
    if parts.len() != 4 {
        return Err(usage);
    }
    let mut nums = [0i32; 4];
    for (i, part) in parts.iter().enumerate() {
        nums[i] = part.parse::<i32>().map_err(|_| usage.clone())?;
    }
    let [x, y, w, h] = nums;
    if w < 1
        || h < 1
        || w > 16384
        || h > 16384
        || !(-16384..=16384).contains(&x)
        || !(-16384..=16384).contains(&y)
    {
        return Err("refuse: move rect out of bounds".to_owned());
    }
    Ok(MoveTarget {
        hwnd,
        tag: args[2].clone(),
        x,
        y,
        w,
        h,
    })
}

/// Parse `close HWND [--tag TAG]` trailing arguments. The bare form stays for
/// the WinArrow harness; proof close/recovery paths always pass the captured
/// `--tag` so a recycled HWND can never be closed by mistake.
pub fn parse_close_args(args: &[String]) -> Result<(u64, Option<String>), String> {
    let usage = "usage: tiler-test-window close HWND [--tag TAG]".to_owned();
    match args.len() {
        1 => {
            let hwnd = parse_hwnd(&args[0]).ok_or(usage)?;
            Ok((hwnd, None))
        }
        3 => {
            if args[1] != "--tag" || args[2].is_empty() {
                return Err(usage);
            }
            let hwnd = parse_hwnd(&args[0]).ok_or(usage)?;
            Ok((hwnd, Some(args[2].clone())))
        }
        _ => Err(usage),
    }
}

#[cfg(windows)]
pub mod sys {
    use super::{
        HELPER_MAX_SECONDS, TEST_WINDOW_CLASS, TEST_WINDOW_H, TEST_WINDOW_PROP, TEST_WINDOW_W,
        TEST_WINDOW_X, TEST_WINDOW_Y, WindowSnapshot,
    };
    use crate::cloak::CloakOps;
    use crate::ffi::cloak::CloakError;
    use crate::ffi::{
        FfiOwnedTestWindow, check_experimental_gate, cloak_hwnd, cloak_mask_of, foreground_window,
        get_prop, install_win_event_hook_for_pid, is_iconic, is_visible, is_window, owner_of,
        parent_of, post_close, set_prop, set_window_pos, show_window, taskbar_add_tab,
        taskbar_delete_tab, try_class_name, uncloak_hwnd, window_pid, window_rect,
    };
    use crate::lifecycle::exe_paths_equal;
    use crate::model::ProcessIdentity;
    use crate::native::{
        HeldProcess, IdentityError, current_exe_path, current_identity, current_integrity_level,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };

    type SResult<T> = Result<T, super::SnapshotError>;
    fn absent(msg: impl Into<String>) -> super::SnapshotError {
        super::SnapshotError::Absent(msg.into())
    }
    fn failed(msg: impl Into<String>) -> super::SnapshotError {
        super::SnapshotError::Failed(msg.into())
    }

    pub fn sibling_helper_exe() -> SResult<String> {
        let me = current_exe_path().map_err(|e| failed(format!("exe {e}")))?;
        let mut path = std::path::PathBuf::from(&me);
        path.pop();
        path.push("tiler-test-window.exe");
        Ok(path.to_string_lossy().into_owned())
    }

    fn medium_or_fail(pid: u32, held: &HeldProcess) -> SResult<()> {
        let rid = held.integrity().map_err(|e| match e {
            IdentityError::Absent => absent(format!("target pid={pid} absent")),
            other => failed(format!("target integrity {other}")),
        })?;
        if !crate::lifecycle::is_medium_rid(rid) {
            return Err(absent(format!("target integrity {rid} is not medium")));
        }
        Ok(())
    }

    fn class_of(hwnd: u64) -> SResult<String> {
        try_class_name(hwnd).ok_or_else(|| absent("unknown class"))
    }

    fn prop_tag(hwnd: u64) -> SResult<String> {
        let Some(token) = get_prop(hwnd, TEST_WINDOW_PROP) else {
            return Err(absent("missing lifetime property"));
        };
        if token == 0 {
            return Err(absent("zero lifetime property"));
        }
        Ok(format!("{token:016x}"))
    }

    fn rect_of(hwnd: u64) -> SResult<(i32, i32, i32, i32)> {
        let Some(raw) = window_rect(hwnd) else {
            return Err(failed("GetWindowRect failed"));
        };
        Ok((raw.left, raw.top, raw.right, raw.bottom))
    }

    pub fn query_owned(
        hwnd_u64: u64,
        expected_exe: &str,
        expected_sid: &str,
        expected_session: u32,
    ) -> SResult<WindowSnapshot> {
        if hwnd_u64 == 0 {
            return Err(absent("zero hwnd"));
        }
        if !is_window(hwnd_u64) {
            return Err(absent(format!("hwnd={hwnd_u64} absent")));
        }
        let pid = window_pid(hwnd_u64);
        if pid == 0 {
            return Err(absent(format!("hwnd={hwnd_u64} no pid")));
        }
        let held = match HeldProcess::open(pid) {
            Ok(h) => h,
            Err(IdentityError::Absent) => return Err(absent(format!("pid={pid} absent"))),
            Err(e) => return Err(failed(format!("owner {e}"))),
        };
        let ident: ProcessIdentity = held.identity().map_err(|e| match e {
            IdentityError::Absent => absent(format!("pid={pid} absent")),
            other => failed(format!("owner {other}")),
        })?;
        if ident.pid != pid {
            return Err(absent("pid reuse"));
        }
        if ident.user_sid != expected_sid || ident.session_id != expected_session {
            return Err(absent("sid/session mismatch"));
        }
        if !exe_paths_equal(&ident.exe_path, expected_exe) {
            return Err(absent("peer exe mismatch"));
        }
        medium_or_fail(pid, &held)?;
        let class = class_of(hwnd_u64)?;
        if class != TEST_WINDOW_CLASS {
            return Err(absent("class mismatch"));
        }
        // Owned-helper identity: no parent, no owner. The native topmost
        // band (WS_EX_TOPMOST) is approved mutable product state (keep-above
        // float/sticky) and never ownership: exe/class/PID/creation/SID/
        // session/medium/tag above already bind the owner.
        if parent_of(hwnd_u64) != 0 {
            return Err(absent("parented"));
        }
        if owner_of(hwnd_u64) != 0 {
            return Err(absent("owned window"));
        }
        let tag = prop_tag(hwnd_u64)?;
        let visible = is_visible(hwnd_u64);
        let (left, top, right, bottom) = rect_of(hwnd_u64)?;
        Ok(WindowSnapshot {
            hwnd: hwnd_u64,
            process: ident,
            tag,
            visible,
            left,
            top,
            right,
            bottom,
        })
    }

    pub fn helper_precheck() -> SResult<ProcessIdentity> {
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let rid = current_integrity_level().map_err(|e| failed(format!("integrity {e}")))?;
        if !crate::lifecycle::is_medium_rid(rid) {
            return Err(absent(format!("integrity {rid} is not medium")));
        }
        Ok(me)
    }

    fn held_matches(pid: u32, expected: &ProcessIdentity) -> SResult<HeldProcess> {
        let held = match HeldProcess::open(pid) {
            Ok(h) => h,
            Err(IdentityError::Absent) => {
                return Err(absent(format!("helper pid={pid} absent")));
            }
            Err(e) => return Err(failed(format!("helper {e}"))),
        };
        let live: ProcessIdentity = held.identity().map_err(|e| match e {
            IdentityError::Absent => absent(format!("helper pid={pid} absent")),
            other => failed(format!("helper {other}")),
        })?;
        if live != *expected || !held.is_alive() {
            return Err(absent("helper identity changed"));
        }
        Ok(held)
    }

    fn foreground_u64() -> u64 {
        foreground_window()
    }

    fn iconic(hwnd: u64) -> bool {
        is_iconic(hwnd)
    }

    fn pid_of(hwnd_u64: u64) -> Option<u32> {
        let pid = window_pid(hwnd_u64);
        (pid != 0).then_some(pid)
    }

    /// Exact-bound mutation preamble shared by `show`, `minimize`, and
    /// `restore`: peer-exe/sid/session/class/property binding via
    /// [`query_owned`], expected-tag match, foreground-at-entry refusal (a
    /// never-focused helper must not be the foreground window), and a
    /// held-process liveness guard that stays alive across the write and
    /// the post-readback. Returns the verified snapshot plus the held
    /// process; callers must keep the handle alive until after readback.
    /// `SW_SHOWMINNOACTIVE`/`SW_SHOWNOACTIVATE` neither activate nor
    /// reorder: z-order of all other windows is preserved.
    fn bound_target(hwnd_u64: u64, expected_tag: &str) -> SResult<(WindowSnapshot, HeldProcess)> {
        if foreground_u64() == hwnd_u64 {
            return Err(absent("helper is foreground"));
        }
        verify_target(hwnd_u64, expected_tag)
    }

    /// Fresh exact-identity revalidation with no foreground gate: HWND plus
    /// peer exe/class/process (pid + creation)/session/integrity, the opaque
    /// lifetime tag, a held-process liveness guard, and a fresh pid recheck
    /// immediately before the caller writes. Every cloak write path calls
    /// this immediately before its own write; observation polls call it per
    /// slice and stop (without writing) when the identity is gone.
    fn verify_target(hwnd_u64: u64, expected_tag: &str) -> SResult<(WindowSnapshot, HeldProcess)> {
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let snap = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if snap.tag != expected_tag {
            return Err(absent("tag mismatch"));
        }
        let held = held_matches(snap.process.pid, &snap.process)?;
        let fresh = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if fresh.process != snap.process || fresh.tag != snap.tag || fresh.hwnd != snap.hwnd {
            return Err(absent("helper identity changed"));
        }
        // Fresh pid immediately before the caller writes: the held handle
        // plus this recheck close the recycle window.
        if pid_of(hwnd_u64) != Some(snap.process.pid) || !held.is_alive() {
            return Err(absent("helper pid changed"));
        }
        Ok((fresh, held))
    }

    fn bound_readback(hwnd_u64: u64, expected: &WindowSnapshot) -> SResult<WindowSnapshot> {
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let back = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if back.process != expected.process
            || back.tag != expected.tag
            || back.hwnd != expected.hwnd
        {
            return Err(absent("helper identity changed"));
        }
        Ok(back)
    }

    /// First-time admission of a passive (hidden) helper: `SetWindowPos`
    /// show without activation or z-order change, then a visibility
    /// readback. There is no hide operation.
    pub fn show_owned(hwnd_u64: u64, expected_tag: &str) -> SResult<String> {
        let (snap, held) = bound_target(hwnd_u64, expected_tag)?;
        if !set_window_pos(
            snap.hwnd,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOZORDER,
        ) {
            return Err(failed("SetWindowPos show failed"));
        }
        // Held liveness spans the write and the readback below.
        let _ = &held;
        let back = bound_readback(hwnd_u64, &snap)?;
        if !back.visible {
            return Err(failed("show readback not visible"));
        }
        let foreground = foreground_u64();
        if foreground == back.hwnd {
            return Err(failed("show stole focus"));
        }
        Ok(
            serde_json::json!({"shown": back.hwnd, "tag": back.tag, "foreground": foreground})
                .to_string(),
        )
    }

    /// Minimize without activating: the foreground window must be untouched.
    pub fn minimize_owned(hwnd_u64: u64, expected_tag: &str) -> SResult<String> {
        let (snap, held) = bound_target(hwnd_u64, expected_tag)?;
        show_window(snap.hwnd, SW_SHOWMINNOACTIVE);
        let _ = &held;
        let back = bound_readback(hwnd_u64, &snap)?;
        if !iconic(back.hwnd) {
            return Err(failed("minimize readback not iconic"));
        }
        let foreground = foreground_u64();
        if foreground == back.hwnd {
            return Err(failed("minimize stole focus"));
        }
        Ok(
            serde_json::json!({"minimized": back.hwnd, "tag": back.tag, "foreground": foreground})
                .to_string(),
        )
    }

    /// Restore a minimized helper without activating it.
    pub fn restore_owned(hwnd_u64: u64, expected_tag: &str) -> SResult<String> {
        let (snap, held) = bound_target(hwnd_u64, expected_tag)?;
        show_window(snap.hwnd, SW_SHOWNOACTIVATE);
        let _ = &held;
        let back = bound_readback(hwnd_u64, &snap)?;
        if iconic(back.hwnd) || !back.visible {
            return Err(failed("restore readback not restored"));
        }
        let foreground = foreground_u64();
        if foreground == back.hwnd {
            return Err(failed("restore stole focus"));
        }
        Ok(
            serde_json::json!({"restored": back.hwnd, "tag": back.tag, "foreground": foreground})
                .to_string(),
        )
    }

    /// Exact-bound location move without activation or z-order change:
    /// `SetWindowPos` with `SWP_NOACTIVATE | SWP_NOZORDER`, then a fresh
    /// geometry readback. Owned helper only; never a foreign window.
    pub fn move_owned(target: &super::MoveTarget) -> SResult<String> {
        let (snap, held) = bound_target(target.hwnd, &target.tag)?;
        if !set_window_pos(
            snap.hwnd,
            target.x,
            target.y,
            target.w,
            target.h,
            SWP_NOACTIVATE | SWP_NOZORDER,
        ) {
            return Err(failed("SetWindowPos move failed"));
        }
        let _ = &held;
        let back = bound_readback(target.hwnd, &snap)?;
        if back.left != target.x
            || back.top != target.y
            || back.right - back.left != target.w
            || back.bottom - back.top != target.h
        {
            return Err(failed("move readback rect mismatch"));
        }
        let foreground = foreground_u64();
        if foreground == back.hwnd {
            return Err(failed("move stole focus"));
        }
        Ok(serde_json::json!({
            "moved": back.hwnd,
            "tag": back.tag,
            "rect": [back.left, back.top, back.right, back.bottom],
            "foreground": foreground,
        })
        .to_string())
    }

    pub fn close_owned(hwnd_u64: u64, expected_tag: Option<&str>) -> SResult<String> {
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let snap = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if let Some(want) = expected_tag
            && snap.tag != want
        {
            return Err(absent("tag mismatch"));
        }
        let _held = held_matches(snap.process.pid, &snap.process)?;
        let fresh = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if fresh.process != snap.process || fresh.tag != snap.tag || fresh.hwnd != snap.hwnd {
            return Err(absent("helper identity changed"));
        }
        if let Some(want) = expected_tag
            && fresh.tag != want
        {
            return Err(absent("tag mismatch"));
        }
        if !post_close(fresh.hwnd) {
            return Err(failed("PostMessage WM_CLOSE failed"));
        }
        Ok(serde_json::json!({"closed": fresh.hwnd, "tag": fresh.tag}).to_string())
    }

    pub fn inspect_owned(hwnd_u64: u64) -> SResult<String> {
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let snap = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        serde_json::to_string(&snap).map_err(|e| failed(format!("json {e}")))
    }

    pub fn run_owned_window(
        receipt: &std::path::Path,
        seconds: u64,
        passive: bool,
    ) -> SResult<String> {
        if seconds == 0 || seconds > HELPER_MAX_SECONDS {
            return Err(absent(format!("seconds must be 1..={HELPER_MAX_SECONDS}")));
        }
        let me = helper_precheck()?;
        let parent = receipt
            .parent()
            .ok_or_else(|| failed("receipt no parent"))?;
        if !parent.is_dir() {
            return Err(failed("receipt parent missing"));
        }
        if receipt.exists() {
            return Err(absent("receipt preexists"));
        }
        if passive {
            // Passive helpers are measured by proof (`GetDpiForWindow` must
            // report the physical 120 DPI): verify Per-Monitor-V2 awareness
            // before any class registration, window creation, or geometry.
            // The default spike path stays legacy unaware and unchanged.
            crate::tiling_sys::ensure_pm_v2().map_err(|e| failed(format!("dpi awareness {e}")))?;
        }
        let owned = FfiOwnedTestWindow::create(
            "OmniTiler owned test",
            TEST_WINDOW_X,
            TEST_WINDOW_Y,
            TEST_WINDOW_W,
            TEST_WINDOW_H,
            !passive,
        )
        .map_err(|e| failed(e.message()))?;
        // Nonzero opaque lifetime token, fresh each creation.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let token = if nanos == 0 { 1 } else { nanos };
        if !set_prop(owned.hwnd(), TEST_WINDOW_PROP, token as usize) {
            return Err(failed("SetProp failed"));
        }
        if !passive {
            show_window(owned.hwnd(), SW_SHOWNORMAL);
        }
        // Fresh readback for receipt bounds. Passive helpers stay hidden
        // until an exact-bound `show` admission. The owned window destroys
        // itself on drop, so every failure below tears it down.
        let hwnd_u64 = owned.hwnd();
        let snap = match query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id) {
            Ok(snap) => snap,
            Err(e) => {
                drop(owned);
                return Err(e);
            }
        };
        if snap.visible == passive {
            drop(owned);
            return Err(failed("window visibility mismatch after create"));
        }
        let json = match serde_json::to_string(&snap) {
            Ok(json) => json,
            Err(e) => {
                drop(owned);
                return Err(failed(format!("json {e}")));
            }
        };
        if let Err(e) = write_receipt_atomic(receipt, &json) {
            drop(owned);
            return Err(e);
        }
        if let Err(e) = owned.run_message_loop((seconds * 1000) as u32) {
            drop(owned);
            return Err(failed(e.message()));
        }
        Ok(json)
    }

    // ---------- experimental private-cloak spike (test helper only) ----------

    fn cloak_gate() -> SResult<u32> {
        check_experimental_gate().map_err(|e| match e {
            CloakError::UnsupportedArch | CloakError::UnsupportedBuild { .. } => {
                absent(e.to_string())
            }
            other => failed(other.to_string()),
        })
    }

    /// Best-effort state read for reports: every field is fail-closed and a
    /// geometry failure stays `None`, never a fake rect. Identity binding is
    /// the caller's job (`verify_target` immediately before any write).
    fn state_of(hwnd_u64: u64) -> crate::cloak::CloakStateReport {
        let mask = cloak_mask_of(hwnd_u64);
        let rect = match rect_of(hwnd_u64) {
            Ok(raw) => Some([raw.0, raw.1, raw.2, raw.3]),
            Err(_) => None,
        };
        crate::cloak::CloakStateReport::new(
            hwnd_u64,
            is_visible(hwnd_u64),
            is_iconic(hwnd_u64),
            mask,
            rect,
            foreground_u64(),
        )
    }

    /// Shared flushed JSONL log plus event counter for one controller or
    /// observer run.
    struct CloakLog {
        inner: std::sync::Mutex<crate::cloak::JsonlLog>,
        events: std::sync::atomic::AtomicU64,
    }

    impl CloakLog {
        fn append(&self, value: &serde_json::Value) {
            if let Ok(mut guard) = self.inner.lock() {
                guard.append(value);
            }
        }

        fn log_failed(&self) -> bool {
            self.inner.lock().map(|guard| guard.failed).unwrap_or(true)
        }

        fn bump_events(&self) {
            self.events
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        fn event_count(&self) -> u64 {
            self.events.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// Live [`crate::cloak::CloakOps`]: `original` is the exact process
    /// identity captured at entry verification and `held` is the open handle
    /// from that same verification, retained across the whole operation.
    /// Every `verify` compares the fresh snapshot against the original (a
    /// matching tag alone never suffices) and rechecks the retained handle's
    /// liveness plus the HWND's pid; `write` repeats that recheck immediately
    /// before the raw native operation. `log` is `None` for standalone
    /// recovery, which reports on stdout instead (and therefore never skips
    /// writes for log health).
    struct LiveOps {
        hwnd: u64,
        tag: String,
        original: ProcessIdentity,
        held: HeldProcess,
        log: Option<std::sync::Arc<CloakLog>>,
    }

    impl crate::cloak::CloakOps for LiveOps {
        fn verify(&mut self) -> Result<crate::cloak::CloakStateReport, crate::cloak::VerifyFault> {
            let me = current_identity()
                .map_err(|e| crate::cloak::VerifyFault(format!("identity {e}")))?;
            let fresh = query_owned(self.hwnd, &me.exe_path, &me.user_sid, me.session_id)
                .map_err(|e| crate::cloak::VerifyFault(e.to_string()))?;
            if fresh.tag != self.tag {
                return Err(crate::cloak::VerifyFault("tag mismatch".to_owned()));
            }
            if fresh.process != self.original {
                return Err(crate::cloak::VerifyFault(
                    "helper identity changed".to_owned(),
                ));
            }
            if pid_of(self.hwnd) != Some(self.original.pid) || !self.held.is_alive() {
                return Err(crate::cloak::VerifyFault("helper pid changed".to_owned()));
            }
            Ok(state_of(fresh.hwnd))
        }

        fn write(&mut self, op: crate::cloak::WriteOp) -> Result<(), String> {
            // Recheck the exact original identity and the retained handle's
            // liveness immediately before the raw native operation: the
            // sequencer verified first, and this closes the gap to the call.
            let me = current_identity().map_err(|e| format!("identity {e}"))?;
            let fresh = query_owned(self.hwnd, &me.exe_path, &me.user_sid, me.session_id)
                .map_err(|e| e.to_string())?;
            if fresh.tag != self.tag || fresh.process != self.original {
                return Err("helper identity changed".to_owned());
            }
            if pid_of(self.hwnd) != Some(self.original.pid) || !self.held.is_alive() {
                return Err("helper pid changed".to_owned());
            }
            match op {
                crate::cloak::WriteOp::Cloak => cloak_hwnd(self.hwnd).map_err(|e| e.to_string()),
                crate::cloak::WriteOp::DeleteTab => {
                    taskbar_delete_tab(self.hwnd).map_err(|e| e.to_string())
                }
                crate::cloak::WriteOp::Uncloak => {
                    uncloak_hwnd(self.hwnd).map_err(|e| e.to_string())
                }
                crate::cloak::WriteOp::AddTab => {
                    taskbar_add_tab(self.hwnd).map_err(|e| e.to_string())
                }
            }
        }

        fn log(&mut self, line: serde_json::Value) -> bool {
            if let Some(log) = &self.log {
                log.append(&line);
                !log.log_failed()
            } else {
                true
            }
        }
    }

    /// Read-only exact-bound preflight: gate, identity, baseline, and the
    /// independent-recovery route. Allows foreground (it only reports
    /// `is_target_foreground`). Success asserts nothing about recovery
    /// working: it instructs the baseline uncloak call that proves it.
    pub fn cloak_preflight(hwnd_u64: u64, expected_tag: &str) -> SResult<String> {
        let build = cloak_gate()?;
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let snap = query_owned(hwnd_u64, &me.exe_path, &me.user_sid, me.session_id)?;
        if snap.tag != expected_tag {
            return Err(absent("tag mismatch"));
        }
        let state = state_of(hwnd_u64);
        crate::cloak::validate_baseline(state.visible, state.iconic, state.cloak_mask, state.rect)
            .map_err(absent)?;
        Ok(serde_json::json!({
            "preflight": "ok",
            "build": build,
            "arch": std::env::consts::ARCH,
            "hwnd": snap.hwnd,
            "tag": snap.tag,
            "process": snap.process,
            "state": state,
            "recovery": format!("candidate independent recovery route (foreground-safe): uncloak {} --tag {} [--added-tab]; verify the baseline call before cloak, then verify recovery after controller exit", snap.hwnd, snap.tag),
        })
        .to_string())
    }

    /// Observation-only sink: exact-HWND events only, no input recording.
    /// Each delivery compares the fresh snapshot against the original
    /// process identity captured at entry (a matching tag alone never
    /// suffices) and logs an EVENT line carrying the OS event id/time;
    /// identity loss is logged honestly with no state. The sink retains no
    /// raw handle: the original [`ProcessIdentity`] is sufficient.
    struct CloakSink {
        hwnd: u64,
        tag: String,
        original: ProcessIdentity,
        start: std::time::Instant,
        seq: std::sync::atomic::AtomicU64,
        log: std::sync::Arc<CloakLog>,
    }

    impl CloakSink {
        fn push_verified(&self, kind: &str, event_id: Option<u32>, os_time: Option<u32>) {
            let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let me = match current_identity() {
                Ok(me) => me,
                Err(e) => {
                    self.log.bump_events();
                    self.log
                        .append(&crate::cloak::event_line(crate::cloak::EventLine {
                            seq,
                            elapsed_ms: self.start.elapsed().as_millis() as u64,
                            kind,
                            event_id,
                            os_time,
                            state: None,
                            verified: false,
                            detail: &format!("identity {e}"),
                        }));
                    return;
                }
            };
            let (state, verified, detail) =
                match query_owned(self.hwnd, &me.exe_path, &me.user_sid, me.session_id) {
                    Ok(snap) if snap.tag == self.tag && snap.process == self.original => {
                        (Some(state_of(snap.hwnd)), true, String::new())
                    }
                    Ok(_) => (None, false, "helper identity changed".to_owned()),
                    Err(e) => (None, false, e.to_string()),
                };
            self.log.bump_events();
            self.log
                .append(&crate::cloak::event_line(crate::cloak::EventLine {
                    seq,
                    elapsed_ms: self.start.elapsed().as_millis() as u64,
                    kind,
                    event_id,
                    os_time,
                    state: state.as_ref(),
                    verified,
                    detail: &detail,
                }));
        }
    }

    impl crate::ffi::WinEventSink for CloakSink {
        fn on_win_event(&self, event: crate::ffi::WinEvent) {
            // Exact-HWND gate first: unrelated windows never enter the log.
            if !crate::cloak::event_is_relevant(event.hwnd, self.hwnd) {
                return;
            }
            self.push_verified(
                crate::cloak::hook_event_kind(event.event),
                Some(event.event),
                Some(event.time),
            );
        }
    }

    /// PID-scoped hooks for foreground + cloak transitions only: the OS
    /// delivers helper-process events to the sink. The existing all-process
    /// wrapper is untouched; no global input hooks are installed.
    fn install_cloak_hooks(
        sink: std::sync::Arc<CloakSink>,
        pid: u32,
    ) -> SResult<Vec<crate::ffi::WinEventHook>> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND,
        };
        crate::ffi::set_win_event_sink(sink);
        let hooks = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ]
        .into_iter()
        .map(|(min, max)| install_win_event_hook_for_pid(min, max, pid))
        .collect::<Option<Vec<_>>>();
        hooks.ok_or_else(|| failed("SetWinEventHook failed"))
    }

    /// Hook + poll observation: hook deliveries flow through the thread pump
    /// while a 500ms poll supplements them with verified state reads. Every
    /// poll revalidates the identity; on identity loss the loop stops without
    /// writing, and on stop-file appearance it records the stop source.
    /// Returns the stop source: `"timeout"`, `"stop-file"`, or
    /// `"identity-lost"`. Polling alone never claims event completeness.
    fn observe_seconds(
        ops: &mut LiveOps,
        sink: &std::sync::Arc<CloakSink>,
        _hooks: &[crate::ffi::WinEventHook],
        seconds: u64,
        stop_file: &std::path::Path,
    ) -> String {
        let slices = seconds.saturating_mul(2).max(2);
        for _ in 0..slices {
            crate::ffi::drain_messages();
            if sink.log.log_failed() {
                return "log-failed".to_owned();
            }
            if crate::cloak::stop_requested(stop_file) {
                return "stop-file".to_owned();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            crate::ffi::drain_messages();
            if crate::cloak::stop_requested(stop_file) {
                return "stop-file".to_owned();
            }
            match ops.verify() {
                Ok(state) => {
                    let seq = sink.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    sink.log.bump_events();
                    sink.log
                        .append(&crate::cloak::event_line(crate::cloak::EventLine {
                            seq,
                            elapsed_ms: sink.start.elapsed().as_millis() as u64,
                            kind: "poll",
                            event_id: None,
                            os_time: None,
                            state: Some(&state),
                            verified: true,
                            detail: "",
                        }));
                }
                Err(fault) => {
                    let seq = sink.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    sink.log.bump_events();
                    sink.log
                        .append(&crate::cloak::event_line(crate::cloak::EventLine {
                            seq,
                            elapsed_ms: sink.start.elapsed().as_millis() as u64,
                            kind: "poll",
                            event_id: None,
                            os_time: None,
                            state: None,
                            verified: false,
                            detail: &fault.0,
                        }));
                    return "identity-lost".to_owned();
                }
            }
        }
        crate::ffi::drain_messages();
        "timeout".to_owned()
    }

    /// Finite controller: baseline, `SetCloak(1, 2)`, optional `DeleteTab`
    /// pairing, PID-scoped observation, then independently verified restore
    /// (`Uncloak`, plus `AddTab` when the obligation was marked). Every write
    /// is freshly verified immediately before it; any attempted mutation is
    /// followed by a restore attempt with aggregated errors, regardless of
    /// prior failures. Refuses foreground at entry like other mutations.
    /// This binary is a GUI-subsystem process, so no console handler exists:
    /// the mandatory stop file is the graceful-stop path, and the log file
    /// carries the write-ahead evidence. `uncloak` remains the standalone
    /// recovery.
    pub fn cloak_owned(args: &crate::cloak::CloakArgs) -> SResult<String> {
        let build = cloak_gate()?;
        crate::cloak::stop_file_armed(&args.stop_file).map_err(absent)?;
        let log = std::sync::Arc::new(CloakLog {
            inner: std::sync::Mutex::new(
                crate::cloak::JsonlLog::create(&args.log).map_err(absent)?,
            ),
            events: std::sync::atomic::AtomicU64::new(0),
        });
        // Entry gates before START: exact identity, foreground refusal, and
        // the visible/uncloaked/non-iconic/geometric baseline.
        let (snap, held) = bound_target(args.hwnd, &args.tag)?;
        // The entry-verified original identity and its held handle are
        // retained for the whole run: every later verify compares against
        // this original, and the handle spans the operation.
        let mut ops = LiveOps {
            hwnd: args.hwnd,
            tag: args.tag.clone(),
            original: snap.process.clone(),
            held,
            log: Some(log.clone()),
        };
        let baseline = state_of(snap.hwnd);
        crate::cloak::validate_baseline(
            baseline.visible,
            baseline.iconic,
            baseline.cloak_mask,
            baseline.rect,
        )
        .map_err(absent)?;
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        let intended: Vec<&str> = if args.delete_tab {
            vec!["cloak", "delete-tab", "uncloak", "add-tab"]
        } else {
            vec!["cloak", "uncloak"]
        };
        log.append(&crate::cloak::start_line(crate::cloak::StartLine {
            controller: &me,
            target: &snap.process,
            hwnd: snap.hwnd,
            tag: &snap.tag,
            baseline: &baseline,
            build,
            variant: "cloak",
            intended_ops: &intended,
            recovery: &format!("uncloak {} --tag {} [--added-tab]", snap.hwnd, snap.tag),
        }));
        if log.log_failed() {
            return Err(failed("log write failed before mutation"));
        }
        let sink = std::sync::Arc::new(CloakSink {
            hwnd: snap.hwnd,
            tag: snap.tag.clone(),
            original: snap.process.clone(),
            start: std::time::Instant::now(),
            seq: std::sync::atomic::AtomicU64::new(0),
            log: log.clone(),
        });
        let hooks = match install_cloak_hooks(sink.clone(), snap.process.pid) {
            Ok(hooks) => hooks,
            Err(e) => {
                log.append(&serde_json::json!({
                    "line": "end",
                    "variant": "cloak",
                    "detail": format!("hook install failed before mutation: {e}"),
                    "errors": [e.to_string()],
                }));
                return Err(e);
            }
        };
        let part = crate::cloak::cloak_part(&mut ops, args.delete_tab);
        let mut errors = part.errors();
        let after_cloak = part.cloak.after.clone();
        // Observation only runs on a clean cloak: attempted, error-free,
        // with a readable nonzero cloak mask. Any other attempted-mutation
        // outcome restores immediately.
        let mut stop_source = "cloak-failed".to_owned();
        let mut restore = None;
        if part.mutation_attempted() {
            if errors.is_empty()
                && !log.log_failed()
                && matches!(&after_cloak, Some(state) if state.cloak_mask.is_some_and(|m| m != 0))
            {
                stop_source =
                    observe_seconds(&mut ops, &sink, &hooks, args.seconds, &args.stop_file);
            }
            let restored = crate::cloak::restore_part(&mut ops, part.tab_obligation);
            errors.extend(restored.errors());
            restore = Some(restored);
        }
        // Final restoration evidence: a fresh verify after all steps, so the
        // report never reuses a stale pre-restore readback. Absence is
        // reported as unreadable, never papered over.
        let restored_state = ops.verify().ok();
        if log.log_failed() {
            errors.push(
                "log write failed during run; persisted prefix is in the log file".to_owned(),
            );
        }
        let mut mismatches = match &restored_state {
            Some(state) => crate::cloak::restoration_mismatches(
                baseline.rect,
                state.rect,
                baseline.visible,
                state.visible,
                state.cloak_mask,
            ),
            None => vec!["restoration unreadable".to_owned()],
        };
        if part.mutation_attempted() && restored_state.is_none() {
            mismatches.push("owned residue: restore writes refused or failed; independent recovery: uncloak with the logged HWND+tag".to_owned());
        }
        log.append(&serde_json::json!({
            "line": "end",
            "variant": "cloak",
            "hwnd": snap.hwnd,
            "stop_source": stop_source,
            "after_cloak": after_cloak,
            "restore": restore,
            "mismatches": mismatches,
            "errors": errors,
            "event_count": log.event_count(),
            "log_failed": log.log_failed(),
        }));
        let report = serde_json::json!({
            "controller": me,
            "build": build,
            "hwnd": snap.hwnd,
            "tag": snap.tag,
            "delete_tab": args.delete_tab,
            "stop_source": stop_source,
            "baseline": baseline,
            "after_cloak": after_cloak,
            "restored": restored_state,
            "mismatches": mismatches,
            "errors": errors,
            "event_count": log.event_count(),
            "log": args.log,
            "log_failed": log.log_failed(),
            "foreground_note": "uncloak allows a foreground target after user selection",
        });
        if errors.is_empty() && mismatches.is_empty() {
            Ok(report.to_string())
        } else {
            Err(failed(format!(
                "cloak run failed (see log {}): errors={errors:?} mismatches={mismatches:?}",
                args.log.display(),
            )))
        }
    }

    /// Standalone recovery: exact HWND + tag, foreground-safe (user
    /// selection may have foregrounded the helper). The private
    /// `SetCloak(1, 0)` call always runs on the freshly verified target,
    /// even on an uncloaked baseline: that baseline call is the
    /// independent-recovery proof, not a no-op bypass. `AddTab` only with
    /// the explicit `--added-tab` flag.
    pub fn uncloak_owned(args: &crate::cloak::UncloakArgs) -> SResult<String> {
        let build = cloak_gate()?;
        // Entry identity for the report (no foreground refusal: recovery
        // must work on a foregrounded target). The entry-verified original
        // and its held handle are retained for the restore that follows.
        let (snap, held) = verify_target(args.hwnd, &args.tag)?;
        let mut ops = LiveOps {
            hwnd: args.hwnd,
            tag: args.tag.clone(),
            original: snap.process.clone(),
            held,
            log: None,
        };
        let before = state_of(snap.hwnd);
        let baseline_call = before.cloak_mask == Some(0);
        let part = crate::cloak::restore_part(&mut ops, args.added_tab);
        let errors = part.errors();
        // Final evidence: a fresh verify after all steps, never a stale
        // pre-restore readback.
        let after = ops.verify().ok();
        let report = serde_json::json!({
            "uncloaked": snap.hwnd,
            "tag": snap.tag,
            "build": build,
            "baseline_call": baseline_call,
            "added_tab": args.added_tab,
            "before": before,
            "after": after,
            "process": snap.process,
            "errors": errors,
        });
        if errors.is_empty() {
            Ok(report.to_string())
        } else {
            Err(failed(format!("uncloak failed: {errors:?}")))
        }
    }

    /// Observation-only command: identity-bound PID-scoped event logging plus
    /// verified poll snapshots, no cloak writes ever.
    pub fn cloak_observe(args: &crate::cloak::ObserveArgs) -> SResult<String> {
        let build = cloak_gate()?;
        crate::cloak::stop_file_armed(&args.stop_file).map_err(absent)?;
        let log = std::sync::Arc::new(CloakLog {
            inner: std::sync::Mutex::new(
                crate::cloak::JsonlLog::create(&args.log).map_err(absent)?,
            ),
            events: std::sync::atomic::AtomicU64::new(0),
        });
        // Read-only entry: exact identity, no foreground gate. The
        // entry-verified original and its held handle are retained for the
        // whole run.
        let (snap, held) = verify_target(args.hwnd, &args.tag)?;
        let mut ops = LiveOps {
            hwnd: args.hwnd,
            tag: args.tag.clone(),
            original: snap.process.clone(),
            held,
            log: Some(log.clone()),
        };
        let baseline = state_of(snap.hwnd);
        let me = current_identity().map_err(|e| failed(format!("identity {e}")))?;
        log.append(&crate::cloak::start_line(crate::cloak::StartLine {
            controller: &me,
            target: &snap.process,
            hwnd: snap.hwnd,
            tag: &snap.tag,
            baseline: &baseline,
            build,
            variant: "observe",
            intended_ops: &["observe"],
            recovery: &format!("uncloak {} --tag {} [--added-tab]", snap.hwnd, snap.tag),
        }));
        if log.log_failed() {
            return Err(failed("log write failed before observation"));
        }
        let sink = std::sync::Arc::new(CloakSink {
            hwnd: snap.hwnd,
            tag: snap.tag.clone(),
            original: snap.process.clone(),
            start: std::time::Instant::now(),
            seq: std::sync::atomic::AtomicU64::new(0),
            log: log.clone(),
        });
        let hooks = match install_cloak_hooks(sink.clone(), snap.process.pid) {
            Ok(hooks) => hooks,
            Err(e) => {
                log.append(&serde_json::json!({
                    "line": "end",
                    "variant": "observe",
                    "detail": format!("hook install failed: {e}"),
                    "errors": [e.to_string()],
                }));
                return Err(e);
            }
        };
        let stop_source = observe_seconds(&mut ops, &sink, &hooks, args.seconds, &args.stop_file);
        let final_state = ops.verify().ok();
        log.append(&serde_json::json!({
            "line": "end",
            "variant": "observe",
            "hwnd": snap.hwnd,
            "stop_source": stop_source,
            "final": final_state,
            "event_count": log.event_count(),
            "log_failed": log.log_failed(),
        }));
        Ok(serde_json::json!({
            "observed": snap.hwnd,
            "tag": snap.tag,
            "build": build,
            "baseline": baseline,
            "final": final_state,
            "stop_source": stop_source,
            "event_count": log.event_count(),
            "log": args.log,
            "log_failed": log.log_failed(),
        })
        .to_string())
    }

    fn write_receipt_atomic(receipt: &std::path::Path, json: &str) -> SResult<()> {
        let tmp = receipt.with_extension("json.tmp");
        {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            let mut f = opts
                .open(&tmp)
                .map_err(|e| failed(format!("receipt open: {e}")))?;
            use std::io::Write;
            f.write_all(json.as_bytes())
                .map_err(|e| failed(format!("receipt write: {e}")))?;
            f.sync_all()
                .map_err(|e| failed(format!("receipt sync: {e}")))?;
        }
        if receipt.exists() {
            let _ = std::fs::remove_file(&tmp);
            return Err(absent("receipt preexists"));
        }
        std::fs::rename(&tmp, receipt).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            failed(format!("receipt rename: {e}"))
        })
    }
}

#[cfg(test)]
mod move_args_tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn move_args_require_exact_bound_rect() {
        let ok = parse_move_args(&strings(&["123", "--tag", "abc", "--to", "10,20,300,200"]))
            .expect("parsed");
        assert_eq!(ok.hwnd, 123);
        assert_eq!(ok.tag, "abc");
        assert_eq!((ok.x, ok.y, ok.w, ok.h), (10, 20, 300, 200));
        assert!(parse_move_args(&strings(&["123", "--tag", "", "--to", "10,20,300,200"])).is_err());
        assert!(
            parse_move_args(&strings(&["123", "--tag", "abc", "--to", "10,20,0,200"])).is_err()
        );
        assert!(parse_move_args(&strings(&["123", "--tag", "abc", "--to", "10,20"])).is_err());
        assert!(
            parse_move_args(&strings(&["0", "--tag", "abc", "--to", "10,20,300,200"])).is_err()
        );
    }
}
