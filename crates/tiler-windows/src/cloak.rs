#![forbid(unsafe_code)]
//! Experimental private-cloak spike: pure parsers, gates, reports, the
//! verified-write sequencer, and JSONL log / stop-file scaffolding.
//!
//! No Win32 calls here: native work stays in `crate::ffi::cloak` and the
//! live [`CloakOps`] implementation in `crate::test_window::sys`. This
//! module is cross-platform so offline unit tests run anywhere, including
//! the operation-seam acceptance tests for the mutation/rollback order.

use serde::Serialize;

/// Hard-coded experimental fence: Windows x64 build 26300 only. This is an
/// experimental fence, not a claim of ABI support.
pub const EXPERIMENTAL_BUILD: u32 = 26300;
/// Expected architecture string for the experimental fence.
pub const EXPERIMENTAL_ARCH: &str = "x86_64";
/// Private `IApplicationView::SetCloak` cloak pair used by the spike.
pub const CLOAK_TYPE: u32 = 1;
/// `SetCloak(1, 2)` cloaks; `SetCloak(1, 0)` uncloaks.
pub const CLOAK_FLAG: i32 = 2;
/// Uncloak flag paired with [`CLOAK_TYPE`].
pub const UNCLOAK_FLAG: i32 = 0;
/// Finite controller/observer bounds (seconds). The helper backstop stays at
/// `HELPER_MAX_SECONDS` (600s); the cloak controller stays well under it.
pub const CONTROLLER_MIN_SECONDS: u64 = 1;
pub const CONTROLLER_MAX_SECONDS: u64 = 120;

/// `DWMWA_CLOAKED` bit meanings for the numeric-mask report.
pub const CLOAK_MASK_APP: u32 = 1;
pub const CLOAK_MASK_SHELL: u32 = 2;
pub const CLOAK_MASK_INHERITED: u32 = 4;

/// Observation completeness disclaimer: polling supplements hooks and never
/// proves event completeness. Hook events carry the OS event id/time; event
/// absence never proves completeness either.
pub const EVENT_COMPLETENESS_NOTE: &str = "polling supplements WinEvent hooks; event absence (hooks or polling) never proves completeness";

/// True only for the hard-coded experimental target.
#[must_use]
pub fn supports_build(build: u32, arch: &str) -> bool {
    build == EXPERIMENTAL_BUILD && arch == EXPERIMENTAL_ARCH
}

/// Closed reason vocabulary for a numeric `DWMWA_CLOAKED` mask.
#[must_use]
pub fn cloak_reason(mask: u32) -> &'static str {
    match mask {
        0 => "none",
        1 => "app",
        2 => "shell",
        3 => "app|shell",
        4 => "inherited",
        5 => "app|inherited",
        6 => "shell|inherited",
        7 => "app|shell|inherited",
        _ => "unknown-bits",
    }
}

/// Baseline gate: visible, non-iconic, readable uncloaked mask, readable
/// geometry. Returns the mask on success; every refusal names the failed
/// condition. A geometry read failure is a refusal, never a fake rect.
pub fn validate_baseline(
    visible: bool,
    iconic: bool,
    mask: Option<u32>,
    rect: Option<[i32; 4]>,
) -> Result<u32, String> {
    let Some(mask) = mask else {
        return Err("refuse: cloak mask unreadable".to_owned());
    };
    if mask != 0 {
        return Err(format!("refuse: preexisting cloak mask={mask}"));
    }
    if rect.is_none() {
        return Err("refuse: baseline geometry unreadable".to_owned());
    }
    if !visible {
        return Err("refuse: baseline not visible".to_owned());
    }
    if iconic {
        return Err("refuse: baseline iconic".to_owned());
    }
    Ok(mask)
}

/// Pure restoration check against the captured baseline. Returns the list of
/// mismatches; empty means geometry, visibility, and cloak all restored.
/// Unreadable geometry on either side is a mismatch, never an assumed match.
#[must_use]
pub fn restoration_mismatches(
    baseline_rect: Option<[i32; 4]>,
    restored_rect: Option<[i32; 4]>,
    baseline_visible: bool,
    restored_visible: bool,
    restored_mask: Option<u32>,
) -> Vec<String> {
    let mut out = Vec::new();
    match (baseline_rect, restored_rect) {
        (Some(a), Some(b)) if a == b => {}
        _ => out.push(format!("rect {baseline_rect:?} != {restored_rect:?}")),
    }
    if baseline_visible != restored_visible {
        out.push(format!("visible {baseline_visible} != {restored_visible}"));
    }
    match restored_mask {
        Some(0) => {}
        Some(m) => out.push(format!("cloak mask={m}")),
        None => out.push("cloak mask unreadable".to_owned()),
    }
    out
}

/// Exact-window event relevance: only the bound target HWND is logged.
#[must_use]
pub fn event_is_relevant(event_hwnd: u64, target_hwnd: u64) -> bool {
    target_hwnd != 0 && event_hwnd == target_hwnd
}

/// Refuse a stop-file path that already exists: a stale stop from a previous
/// run must never instantly stop a new controller.
pub fn stop_file_armed(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("refuse: stop file preexists: {}", path.display()));
    }
    Ok(())
}

/// Polled accessible stop: the user creates this file to request graceful
/// stop; the controller restores before exiting.
#[must_use]
pub fn stop_requested(path: &std::path::Path) -> bool {
    path.exists()
}

/// Flushed append-only JSONL experiment log. Created with `create_new` so a
/// preexisting path refuses instead of mixing runs. Every line is flushed
/// before the next native call, so a controller crash still leaves the
/// initial identity and the latest after-step evidence on disk.
pub struct JsonlLog {
    file: std::fs::File,
    /// Set once any append fails; the run continues (a failed log after a
    /// mutation must not skip restoration) and reports it at the end.
    pub failed: bool,
}

impl JsonlLog {
    /// Fresh log only; a preexisting path (or any open failure) aborts the
    /// run before any mutation.
    pub fn create(path: &std::path::Path) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("refuse: log {} not created fresh: {e}", path.display()))?;
        Ok(Self {
            file,
            failed: false,
        })
    }

    /// Append one compact JSON line, flushed. Failures latch [`Self::failed`]
    /// and never panic.
    pub fn append(&mut self, value: &serde_json::Value) {
        use std::io::Write;
        if self.failed {
            return;
        }
        let line = value.to_string();
        if writeln!(self.file, "{line}").is_err() || self.file.flush().is_err() {
            self.failed = true;
        }
    }
}

/// One verified native write. `wrote` is true whenever the write was
/// attempted, even if it (or the after-readback) failed: a failed private
/// call may still have mutated state, so restoration must be attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WriteOp {
    Cloak,
    DeleteTab,
    Uncloak,
    AddTab,
}

impl WriteOp {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cloak => "cloak",
            Self::DeleteTab => "delete-tab",
            Self::Uncloak => "uncloak",
            Self::AddTab => "add-tab",
        }
    }
}

/// Fresh revalidation failure: the original identity is absent or changed.
/// The caller must not write any handle on this path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyFault(pub String);

/// Injected-operation seam for the verified-write sequencer. The live
/// implementation revalidates the exact original process/tag/HWND/class/exe/
/// session/medium with the retained held handle on every [`Self::verify`]
/// and rechecks it immediately before every [`Self::write`]; tests inject
/// scripted faults to prove the mutation/rollback order.
pub trait CloakOps {
    /// Fresh full revalidation against the original identity plus state read.
    /// Identity loss fails closed; state-read failures are carried as `None`
    /// fields, never fabrications.
    fn verify(&mut self) -> Result<CloakStateReport, VerifyFault>;
    /// One raw native write; the sequencer verifies immediately before and
    /// reads back immediately after every call. The implementation rechecks
    /// the original identity immediately before the raw operation.
    fn write(&mut self, op: WriteOp) -> Result<(), String>;
    /// Persist one JSONL line. Returns false when the line was not
    /// persisted; the sequencer then skips new hide mutations but never
    /// suppresses restoration.
    fn log(&mut self, line: serde_json::Value) -> bool;
}

/// One verified write plus its readbacks. `wrote` tracks whether the native
/// call was attempted; `error` aggregates the first failure, if any.
/// `log_ok` tracks whether every STEP line for this attempt was persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteAttempt {
    pub op: &'static str,
    pub wrote: bool,
    pub before: Option<CloakStateReport>,
    pub after: Option<CloakStateReport>,
    pub error: Option<String>,
    pub log_ok: bool,
}

/// Verify, write once, read back: the only shape that ever mutates a handle.
/// A STEP line is logged for every outcome, including identity loss. The
/// `phase` of each logged state names which side of the write it came from:
/// `"before"` for the pre-write readback, `"after"` for the post-write
/// readback, `"none"` when identity was absent. A raw-operation failure
/// still performs the verified post-readback so an HRESULT failure that
/// mutated state leaves honest after-state; the pre-write state is never
/// logged as if it were the after-state.
pub fn verified_write<O: CloakOps>(ops: &mut O, op: WriteOp) -> WriteAttempt {
    let before = match ops.verify() {
        Ok(state) => Some(state),
        Err(fault) => {
            let detail = format!("identity lost before {}: {}", op.name(), fault.0);
            let logged = ops.log(step_line(op.name(), false, None, "none", &detail));
            return WriteAttempt {
                op: op.name(),
                wrote: false,
                before: None,
                after: None,
                error: Some(detail),
                log_ok: logged,
            };
        }
    };
    if let Err(e) = ops.write(op) {
        let detail = format!("{} failed: {e}", op.name());
        let mut logged = ops.log(step_line(
            op.name(),
            false,
            before.as_ref(),
            "before",
            &detail,
        ));
        let after = ops.verify().ok();
        if let Some(state) = &after {
            logged &= ops.log(step_line(op.name(), false, Some(state), "after", &detail));
        }
        return WriteAttempt {
            op: op.name(),
            wrote: true,
            before,
            after,
            error: Some(detail),
            log_ok: logged,
        };
    }
    match ops.verify() {
        Ok(after) => {
            let logged = ops.log(step_line(op.name(), true, Some(&after), "after", ""));
            WriteAttempt {
                op: op.name(),
                wrote: true,
                before,
                after: Some(after),
                error: None,
                log_ok: logged,
            }
        }
        Err(fault) => {
            let detail = format!("identity lost after {}: {}", op.name(), fault.0);
            let logged = ops.log(step_line(
                op.name(),
                false,
                before.as_ref(),
                "before",
                &detail,
            ));
            WriteAttempt {
                op: op.name(),
                wrote: true,
                before,
                after: None,
                error: Some(detail),
                log_ok: logged,
            }
        }
    }
}

/// Controller mutation half: `SetCloak(1, 2)` then the optional `DeleteTab`
/// pairing. `DeleteTab` runs only after a clean cloak: attempted,
/// error-free, logged, with a readable nonzero cloak mask. The taskbar
/// obligation is marked immediately before the actual `DeleteTab` attempt so
/// a crash still leaves the outstanding obligation on disk; a failed
/// obligation mark skips the pairing but never the restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloakPart {
    pub cloak: WriteAttempt,
    pub delete_tab: Option<WriteAttempt>,
    /// True once the add-tab obligation was marked immediately before an
    /// actual `DeleteTab` attempt: restoration must attempt `AddTab`
    /// whenever this is set.
    pub tab_obligation: bool,
}

impl CloakPart {
    /// Every failure that must surface, in order. Restoration is decided by
    /// the caller from [`Self::mutation_attempted`], not by this list.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.cloak.error {
            out.push(e.clone());
        } else if self.cloak.wrote {
            match &self.cloak.after {
                Some(state) if state.cloak_mask.is_none_or(|m| m == 0) => {
                    out.push("cloak readback mask still none".to_owned());
                }
                None => out.push("cloak readback unreadable".to_owned()),
                _ => {}
            }
        }
        if let Some(attempt) = &self.delete_tab
            && let Some(e) = &attempt.error
        {
            out.push(e.clone());
        }
        out
    }

    /// True once the native cloak call was attempted, however it ended: a
    /// failed private call may still have mutated state.
    #[must_use]
    pub const fn mutation_attempted(&self) -> bool {
        self.cloak.wrote
    }
}

/// Run the controller mutation half over injected operations. A failed,
/// unreadable, or non-cloaking cloak readback skips the pairing: no new
/// hide mutation follows uncertain cloak state.
pub fn cloak_part<O: CloakOps>(ops: &mut O, pair_tab: bool) -> CloakPart {
    let cloak = verified_write(ops, WriteOp::Cloak);
    let cloak_clean = cloak.wrote
        && cloak.error.is_none()
        && cloak.log_ok
        && matches!(&cloak.after, Some(state) if state.cloak_mask.is_some_and(|m| m != 0));
    let mut delete_tab = None;
    let mut tab_obligation = false;
    if pair_tab && cloak_clean {
        // Mark immediately before the actual attempt; a failed mark skips
        // the pairing but never suppresses the restore.
        if ops.log(obligation_line(
            "add-tab",
            "DeleteTab pairing attempted; AddTab restoration is owed",
        )) {
            tab_obligation = true;
            delete_tab = Some(verified_write(ops, WriteOp::DeleteTab));
        }
    }
    CloakPart {
        cloak,
        delete_tab,
        tab_obligation,
    }
}

/// Restoration half, shared by the controller and standalone recovery:
/// independent `Uncloak` first, then `AddTab` whenever the obligation is set.
/// Both are attempted (each freshly verified) and every failure is
/// aggregated; an `AddTab` failure never skips the `Uncloak` because the
/// uncloak runs first, and an uncloak failure never skips a owed `AddTab`.
/// Restoration is a separate verified operation, never a retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePart {
    pub uncloak: WriteAttempt,
    pub add_tab: Option<WriteAttempt>,
}

impl RestorePart {
    /// Every failure that must surface, in order.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.uncloak.error {
            out.push(e.clone());
        } else if self.uncloak.wrote {
            match &self.uncloak.after {
                Some(state) if state.cloak_mask != Some(0) => {
                    out.push(format!(
                        "uncloak readback mask not none: {:?}",
                        state.cloak_mask
                    ));
                }
                None => out.push("uncloak readback unreadable".to_owned()),
                _ => {}
            }
        }
        if let Some(attempt) = &self.add_tab
            && let Some(e) = &attempt.error
        {
            out.push(e.clone());
        }
        out
    }
}

/// Run the restoration half over injected operations.
pub fn restore_part<O: CloakOps>(ops: &mut O, tab_obligation: bool) -> RestorePart {
    let uncloak = verified_write(ops, WriteOp::Uncloak);
    let mut add_tab = None;
    if tab_obligation {
        add_tab = Some(verified_write(ops, WriteOp::AddTab));
    }
    RestorePart { uncloak, add_tab }
}

/// Exact-bound cloak controller arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloakArgs {
    pub hwnd: u64,
    pub tag: String,
    pub seconds: u64,
    pub delete_tab: bool,
    pub stop_file: std::path::PathBuf,
    pub log: std::path::PathBuf,
}

/// Parse `cloak HWND --tag TAG --seconds N [--delete-tab] --stop-file PATH
/// --log PATH`. The stop and log paths are mandatory; a preexisting path
/// refuses at startup (checked by the caller before any mutation).
pub fn parse_cloak_args(args: &[String]) -> Result<CloakArgs, String> {
    let usage = "usage: tiler-test-window cloak HWND --tag TAG --seconds N [--delete-tab] --stop-file PATH --log PATH"
        .to_owned();
    let mut tag: Option<String> = None;
    let mut seconds: Option<u64> = None;
    let mut stop_file: Option<std::path::PathBuf> = None;
    let mut log: Option<std::path::PathBuf> = None;
    let mut delete_tab = false;
    if args.is_empty() {
        return Err(usage);
    }
    let hwnd = crate::test_window::parse_hwnd(&args[0]).ok_or_else(|| usage.clone())?;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--tag" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                tag = Some(v.clone());
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                seconds = Some(v.parse::<u64>().map_err(|_| usage.clone())?);
                i += 1;
            }
            "--delete-tab" => {
                delete_tab = true;
                i += 1;
            }
            "--stop-file" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                stop_file = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            "--log" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                log = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            _ => return Err(usage),
        }
    }
    let (tag, seconds, stop_file, log) = match (tag, seconds, stop_file, log) {
        (Some(t), Some(s), Some(sf), Some(l)) => (t, s, sf, l),
        _ => return Err(usage),
    };
    if !(CONTROLLER_MIN_SECONDS..=CONTROLLER_MAX_SECONDS).contains(&seconds) {
        return Err(format!(
            "refuse: seconds must be {}..={}",
            CONTROLLER_MIN_SECONDS, CONTROLLER_MAX_SECONDS
        ));
    }
    Ok(CloakArgs {
        hwnd,
        tag,
        seconds,
        delete_tab,
        stop_file,
        log,
    })
}

/// Exact-bound standalone recovery arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncloakArgs {
    pub hwnd: u64,
    pub tag: String,
    pub added_tab: bool,
}

/// Parse `uncloak HWND --tag TAG [--added-tab]`. The private uncloak call
/// always runs on the verified target, even on an uncloaked baseline: that
/// baseline call is the independent-recovery proof, not a no-op bypass.
pub fn parse_uncloak_args(args: &[String]) -> Result<UncloakArgs, String> {
    let usage = "usage: tiler-test-window uncloak HWND --tag TAG [--added-tab]".to_owned();
    if args.len() != 3 && args.len() != 4 {
        return Err(usage);
    }
    let hwnd = crate::test_window::parse_hwnd(&args[0]).ok_or_else(|| usage.clone())?;
    if args[1] != "--tag" || args[2].is_empty() {
        return Err(usage);
    }
    let mut added_tab = false;
    if args.len() == 4 {
        if args[3] != "--added-tab" {
            return Err(usage);
        }
        added_tab = true;
    }
    Ok(UncloakArgs {
        hwnd,
        tag: args[2].clone(),
        added_tab,
    })
}

/// Exact-bound observation-only arguments (no cloak writes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveArgs {
    pub hwnd: u64,
    pub tag: String,
    pub seconds: u64,
    pub stop_file: std::path::PathBuf,
    pub log: std::path::PathBuf,
}

/// Parse `cloak-observe HWND --tag TAG --seconds N --stop-file PATH --log
/// PATH`. Observation never mutates; the stop and log paths are still
/// mandatory so a stuck observer is always stoppable and always evidenced.
pub fn parse_observe_args(args: &[String]) -> Result<ObserveArgs, String> {
    let usage =
        "usage: tiler-test-window cloak-observe HWND --tag TAG --seconds N --stop-file PATH --log PATH"
            .to_owned();
    if args.is_empty() {
        return Err(usage);
    }
    let hwnd = crate::test_window::parse_hwnd(&args[0]).ok_or_else(|| usage.clone())?;
    let mut tag: Option<String> = None;
    let mut seconds: Option<u64> = None;
    let mut stop_file: Option<std::path::PathBuf> = None;
    let mut log: Option<std::path::PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--tag" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                tag = Some(v.clone());
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                seconds = Some(v.parse::<u64>().map_err(|_| usage.clone())?);
                i += 1;
            }
            "--stop-file" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                stop_file = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            "--log" => {
                i += 1;
                let v = args.get(i).ok_or_else(|| usage.clone())?;
                if v.is_empty() {
                    return Err(usage);
                }
                log = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            _ => return Err(usage),
        }
    }
    let (tag, seconds, stop_file, log) = match (tag, seconds, stop_file, log) {
        (Some(t), Some(s), Some(sf), Some(l)) => (t, s, sf, l),
        _ => return Err(usage),
    };
    if !(CONTROLLER_MIN_SECONDS..=CONTROLLER_MAX_SECONDS).contains(&seconds) {
        return Err(format!(
            "refuse: seconds must be {}..={}",
            CONTROLLER_MIN_SECONDS, CONTROLLER_MAX_SECONDS
        ));
    }
    Ok(ObserveArgs {
        hwnd,
        tag,
        seconds,
        stop_file,
        log,
    })
}

/// Numeric cloak state report (dedicated; the existing snapshot stays bool).
/// `rect` is `None` when geometry was unreadable: callers refuse or report a
/// mismatch rather than substituting a fake rect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloakStateReport {
    pub hwnd: u64,
    pub visible: bool,
    pub iconic: bool,
    pub cloak_mask: Option<u32>,
    pub cloak_reason: String,
    pub rect: Option<[i32; 4]>,
    pub foreground: u64,
    pub is_target_foreground: bool,
}

impl CloakStateReport {
    #[must_use]
    pub fn new(
        hwnd: u64,
        visible: bool,
        iconic: bool,
        mask: Option<u32>,
        rect: Option<[i32; 4]>,
        foreground: u64,
    ) -> Self {
        let reason = match mask {
            Some(m) => cloak_reason(m).to_owned(),
            None => "unreadable".to_owned(),
        };
        Self {
            hwnd,
            visible,
            iconic,
            cloak_mask: mask,
            cloak_reason: reason,
            rect,
            foreground,
            is_target_foreground: foreground == hwnd,
        }
    }
}

/// One observation-only event for the bound HWND (no input recording).
/// `os_time` carries the OS event time for hook deliveries; poll entries
/// have no OS event and report `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloakEventReport {
    pub seq: u64,
    pub elapsed_ms: u64,
    pub kind: String,
    pub event_id: Option<u32>,
    pub os_time: Option<u32>,
    pub hwnd: u64,
    pub foreground: u64,
    pub cloak_mask: Option<u32>,
    pub visible: bool,
}

/// Closed vocabulary for hook event kinds: foreground, cloak transitions,
/// and poll supplements only.
#[must_use]
pub fn hook_event_kind(event_id: u32) -> &'static str {
    match event_id {
        0x0003 => "foreground",
        0x8017 => "cloaked",
        0x8018 => "uncloaked",
        _ => "winevent",
    }
}

/// START-line parameters: written (flushed) before any mutation. A controller
/// crash after this point still leaves the full controller/target identity
/// plus the intended operations and the independent recovery command on disk.
pub struct StartLine<'a> {
    pub controller: &'a crate::model::ProcessIdentity,
    pub target: &'a crate::model::ProcessIdentity,
    pub hwnd: u64,
    pub tag: &'a str,
    pub baseline: &'a CloakStateReport,
    pub build: u32,
    pub variant: &'a str,
    pub intended_ops: &'a [&'a str],
    pub recovery: &'a str,
}

/// START log line builder.
pub fn start_line(p: StartLine<'_>) -> serde_json::Value {
    serde_json::json!({
        "line": "start",
        "variant": p.variant,
        "build": p.build,
        "arch": EXPERIMENTAL_ARCH,
        "controller": p.controller,
        "target": p.target,
        "hwnd": p.hwnd,
        "tag": p.tag,
        "baseline": p.baseline,
        "intended_ops": p.intended_ops,
        "recovery": p.recovery,
        "abi_note": "experimental ABI not validated; QueryService/IID checks are not proof of layout",
    })
}

/// Obligation marker: logged (persisted) before a `DeleteTab` attempt so a
/// crash still leaves the outstanding `AddTab` obligation evidenced.
pub fn obligation_line(kind: &str, detail: &str) -> serde_json::Value {
    serde_json::json!({
        "line": "obligation",
        "kind": kind,
        "detail": detail,
    })
}

/// STEP log line: one verified write plus the readback it belongs to. The
/// `phase` names which side of the write `state` came from (`"before"`,
/// `"after"`, or `"none"` when identity was absent), so a pre-write state is
/// never mistaken for a post-write one.
pub fn step_line(
    op: &str,
    ok: bool,
    state: Option<&CloakStateReport>,
    phase: &str,
    detail: &str,
) -> serde_json::Value {
    serde_json::json!({
        "line": "step",
        "op": op,
        "ok": ok,
        "phase": phase,
        "state": state,
        "detail": detail,
    })
}

/// EVENT-line parameters: hook deliveries carry the OS event id/time; poll
/// entries carry `verified` state reads. Event absence never proves
/// completeness.
pub struct EventLine<'a> {
    pub seq: u64,
    pub elapsed_ms: u64,
    pub kind: &'a str,
    pub event_id: Option<u32>,
    pub os_time: Option<u32>,
    pub state: Option<&'a CloakStateReport>,
    pub verified: bool,
    pub detail: &'a str,
}

/// EVENT log line builder.
pub fn event_line(p: EventLine<'_>) -> serde_json::Value {
    serde_json::json!({
        "line": "event",
        "seq": p.seq,
        "elapsed_ms": p.elapsed_ms,
        "kind": p.kind,
        "event_id": p.event_id,
        "os_time": p.os_time,
        "verified": p.verified,
        "state": p.state,
        "detail": p.detail,
        "note": EVENT_COMPLETENESS_NOTE,
    })
}
