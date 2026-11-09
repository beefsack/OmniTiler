//! Shared platform-neutral high-volume stress definitions plus case loading
//! and schedule helpers (development-only, never a public API).
//!
//! Two small deterministic single-output zero-gap tiled cases stress sends
//! (follow and stay), workspace switches (explicit and core-resolved),
//! directional focus/moves and admit/close churn with independently authored
//! final oracles. The shared [`StressSchedule`] covers the same actions on
//! ordered, batched and reordered schedules; the active runtime is
//! [`crate::world_stress_v4::run_stress_v4`].
//!
//! Command reports carry the predicted real foreground (never the subject):
//! directional/send subjects must already name that foreground window or the
//! case refuses as not natively honorable. Cases acquire foreground only
//! through explicit `focus-window` observations (fixture focus with readback
//! on the native leg); the driver never refocuses silently and checkpoints
//! distinguish retained core `focus` (the last one-shot effect target, kept
//! while its workspace hides) from expected real `foreground` (null when no
//! managed fixture window can hold it).
//!
//! The Windows PowerShell runner reuses the same JSON files plus
//! [`normalize_tiles_to_work`] and the checkpoint semantics documented in
//! `test-fixtures/world-stress.md`. No product IPC changes.

use serde::Deserialize as _;
use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{FrameSetter, MemberAttempt, WorkspaceRef, WorldCommand};

pub(crate) fn rect4(v: [i32; 4]) -> Rect {
    Rect {
        x: v[0],
        y: v[1],
        w: v[2],
        h: v[3],
    }
}

pub(crate) fn rect_of(v: Rect) -> [i32; 4] {
    [v.x, v.y, v.w, v.h]
}

fn area(r: [i32; 4]) -> i64 {
    i64::from(r[2]) * i64::from(r[3])
}

fn overlap(a: [i32; 4], b: [i32; 4]) -> bool {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    (a[0] + a[2]).min(b[0] + b[2]) > x0 && (a[1] + a[3]).min(b[1] + b[3]) > y0
}

fn inside(outer: [i32; 4], inner: [i32; 4]) -> bool {
    inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2]
        && inner[1] + inner[3] <= outer[1] + outer[3]
}

pub(crate) fn coverage_ok(work: [i32; 4], allocs: &[[i32; 4]]) -> Result<(), String> {
    for a in allocs {
        if a[2] <= 0 || a[3] <= 0 {
            return Err("allocation has non-positive area".to_owned());
        }
        if !inside(work, *a) {
            return Err("allocation escapes the work area".to_owned());
        }
    }
    for (i, a) in allocs.iter().enumerate() {
        for b in allocs.iter().skip(i + 1) {
            if overlap(*a, *b) {
                return Err("allocations overlap".to_owned());
            }
        }
    }
    let sum: i64 = allocs.iter().map(|a| area(*a)).sum();
    if sum != area(work) {
        return Err(format!(
            "allocation area {sum} != work area {} (gap)",
            area(work)
        ));
    }
    Ok(())
}

pub(crate) fn fail(at: &str, msg: impl std::fmt::Display) -> String {
    format!("{at}: {msg}")
}

/// Presence-preserving `foreground` decoding: a missing field decodes to
/// `None` (refused), JSON null to `Some(None)` (no managed foreground),
/// and a window id to `Some(Some(id))`.
fn de_presence<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

// ---------------------------------------------------------------------------
// Case file schema (strict: unknown fields refuse).
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StressScreen {
    pub(crate) out: String,
    pub(crate) full: [i32; 4],
    pub(crate) work: [i32; 4],
    pub(crate) gap: i32,
    pub(crate) outer_gap: i32,
    pub(crate) complete: bool,
    pub(crate) primary: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StressSettings {
    pub(crate) default_tiled: bool,
}

/// One action in the stream. Command actions carry `seq`; observation-only
/// actions (`admit`, `close`, `checkpoint`) carry none. Checkpoint actions
/// carry the full intermediate oracle.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawAction {
    pub(crate) op: String,
    pub(crate) seq: Option<u64>,
    pub(crate) output: Option<String>,
    pub(crate) workspace: Option<String>,
    pub(crate) index: Option<u8>,
    pub(crate) delta: Option<i32>,
    pub(crate) window: Option<String>,
    pub(crate) direction: Option<String>,
    pub(crate) same_axis: Option<String>,
    pub(crate) follow: Option<bool>,
    pub(crate) rect: Option<[i32; 4]>,
    pub(crate) name: Option<String>,
    pub(crate) view: Option<String>,
    pub(crate) members: Option<Vec<String>>,
    pub(crate) homing: Option<BTreeMap<String, String>>,
    pub(crate) tiles: Option<BTreeMap<String, [i32; 4]>>,
    pub(crate) focus: Option<String>,
    /// Expected real foreground: a live window id, or null when no managed
    /// fixture window can hold the foreground (hidden focus never counts).
    /// Present (even null) on every checkpoint; refused when absent.
    #[serde(default, deserialize_with = "de_presence")]
    pub(crate) foreground: Option<Option<String>>,
    pub(crate) expect_effect: Option<String>,
    pub(crate) expect_target: Option<String>,
    pub(crate) pager_current: Option<String>,
    pub(crate) pager_occupied: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinalExpect {
    pub(crate) view: String,
    pub(crate) members: Vec<String>,
    pub(crate) homing: BTreeMap<String, String>,
    pub(crate) tiles: BTreeMap<String, [i32; 4]>,
    pub(crate) focus: String,
    pub(crate) foreground: Option<String>,
    pub(crate) pager_current: String,
    pub(crate) pager_occupied: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StressCase {
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) description: String,
    #[allow(dead_code)]
    pub(crate) owner: String,
    pub(crate) adapter: String,
    pub(crate) generation: String,
    pub(crate) mode: String,
    pub(crate) screens: Vec<StressScreen>,
    pub(crate) settings: StressSettings,
    pub(crate) windows: Vec<String>,
    pub(crate) initial_facts: BTreeMap<String, [i32; 4]>,
    pub(crate) actions: Vec<RawAction>,
    #[serde(rename = "final")]
    pub(crate) final_expect: FinalExpect,
}

pub fn load_stress_case(path: &str) -> Result<StressCase, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read case: {e}"))?;
    let case: StressCase = serde_json::from_str(&text).map_err(|e| format!("parse case: {e}"))?;
    validate_case(&case)?;
    Ok(case)
}

impl StressCase {
    /// Case label for logs; the oracle never reads it.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn is_alias(value: &str) -> bool {
    value == "home" || value == "away"
}

pub(crate) fn validate_case(case: &StressCase) -> Result<(), String> {
    if case.screens.len() != 1 {
        return Err("stress case covers exactly one screen".to_owned());
    }
    let screen = &case.screens[0];
    if screen.gap != 0 || screen.outer_gap != 0 || !screen.complete {
        return Err("only zero-gap complete screens are covered".to_owned());
    }
    if !case.settings.default_tiled {
        return Err("only default_tiled=true is covered".to_owned());
    }
    if case.mode != "per-output-local" {
        return Err(format!("mode {:?} is not covered", case.mode));
    }
    let mut seen = BTreeSet::new();
    for w in &case.windows {
        if !seen.insert(w.clone()) {
            return Err(format!("duplicate window id {w:?}"));
        }
    }
    let initial: BTreeSet<&String> = case.initial_facts.keys().collect();
    let all: BTreeSet<&String> = case.windows.iter().collect();
    if initial != all {
        // Churn windows admit later, so initial facts may name a subset.
        // Every initial fact must name a known window.
        for w in &initial {
            if !all.contains(*w) {
                return Err(format!("initial fact {w:?} names an unknown window"));
            }
        }
        if initial.is_empty() {
            return Err("initial facts must not be empty".to_owned());
        }
    }
    coverage_ok(
        screen.work,
        &case.initial_facts.values().copied().collect::<Vec<_>>(),
    )
    .map_err(|e| format!("initial facts do not tile the work area: {e}"))?;
    if case.actions.len() < 32 {
        return Err(format!("stress volume {} < 32 actions", case.actions.len()));
    }
    let checkpoints = case.actions.iter().filter(|a| a.op == "checkpoint").count();
    if checkpoints < 4 {
        return Err("at least four checkpoints are required".to_owned());
    }
    let mut seqs: Vec<u64> = Vec::new();
    let mut live: BTreeSet<String> = initial.iter().map(|s| (*s).clone()).collect();
    for (i, action) in case.actions.iter().enumerate() {
        let at = format!("action[{i}] {}", action.op);
        validate_action(action, &at, &case.windows)?;
        match action.op.as_str() {
            "admit" => {
                let w = action.window.as_ref().expect("validated");
                if live.contains(w) {
                    return Err(fail(&at, "admit of a live window"));
                }
                live.insert(w.clone());
            }
            "focus-window" => {
                let w = action.window.as_ref().expect("validated");
                if !live.contains(w) {
                    return Err(fail(&at, "focus-window of a non-live window"));
                }
            }
            "close" => {
                let w = action.window.as_ref().expect("validated");
                if !live.remove(w) {
                    return Err(fail(&at, "close of a non-live window"));
                }
            }
            "checkpoint" => {
                let members = action.members.as_ref().expect("validated");
                let live_sorted: Vec<String> = live.iter().cloned().collect();
                let mut want = members.clone();
                want.sort();
                if want != live_sorted {
                    return Err(fail(&at, "checkpoint members != live roster"));
                }
                if action.foreground.is_none() {
                    return Err(fail(&at, "checkpoint needs an explicit foreground"));
                }
                if let Some(Some(fg)) = action.foreground.as_ref()
                    && !case.windows.contains(fg)
                {
                    return Err(fail(&at, "checkpoint foreground names an unknown window"));
                }
            }
            _ => {
                seqs.push(action.seq.expect("validated"));
            }
        }
        // Same-window back-to-back sends without an intervening receipt
        // would hit transfer-in-flight by construction; checkpoints,
        // admit/close and explicit focus observations carry receipts. Count
        // sends for this window back to the previous receipt boundary.
        if action.op == "send" {
            let w = action.window.as_ref().expect("validated");
            let mut count = 0;
            for prev in case.actions[..i].iter().rev() {
                match prev.op.as_str() {
                    "checkpoint" | "admit" | "close" | "focus-window" => break,
                    "send" if prev.window.as_ref() == Some(w) => count += 1,
                    _ => {}
                }
            }
            if count > 0 {
                return Err(fail(&at, "second send for a window without a receipt"));
            }
        }
    }
    let mut sorted = seqs.clone();
    sorted.sort();
    if sorted != seqs || sorted.len() != BTreeSet::from_iter(seqs.iter()).len() {
        return Err("command seqs must be strictly increasing".to_owned());
    }
    if seqs.len() < 100 {
        return Err(format!(
            "stress volume {} < 100 product operations",
            seqs.len()
        ));
    }
    // Final oracle consistency (hand-authored literals, never generated).
    let mut members = case.final_expect.members.clone();
    members.sort();
    let mut homing_keys: Vec<String> = case.final_expect.homing.keys().cloned().collect();
    homing_keys.sort();
    let mut tile_keys: Vec<String> = case.final_expect.tiles.keys().cloned().collect();
    tile_keys.sort();
    if homing_keys != members || tile_keys != members {
        return Err("final homing/tiles must name exactly the members".to_owned());
    }
    for alias in case.final_expect.homing.values() {
        if !is_alias(alias) {
            return Err(format!("final homing alias {alias:?} is not home/away"));
        }
    }
    if !is_alias(&case.final_expect.view) || !is_alias(&case.final_expect.pager_current) {
        return Err("final view/pager_current must be home/away".to_owned());
    }
    for alias in &case.final_expect.pager_occupied {
        if !is_alias(alias) {
            return Err(format!("final pager alias {alias:?} is not home/away"));
        }
    }
    Ok(())
}

fn forbid(action: &RawAction, at: &str, field: &str, present: bool) -> Result<(), String> {
    if present {
        return Err(fail(at, format!("{field} is not covered for this op")));
    }
    let _ = action;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_action(action: &RawAction, at: &str, windows: &[String]) -> Result<(), String> {
    let window_known = |w: &Option<String>| -> Result<(), String> {
        match w {
            Some(id) if windows.contains(id) => Ok(()),
            _ => Err(fail(at, "unknown or missing window")),
        }
    };
    let alias_ok = |w: &Option<String>| -> Result<(), String> {
        match w.as_deref() {
            Some(alias) if is_alias(alias) => Ok(()),
            _ => Err(fail(at, "workspace must name the home/away alias")),
        }
    };
    match action.op.as_str() {
        "select-workspace" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            if action.output.is_none() {
                return Err(fail(at, "select-workspace needs an explicit output"));
            }
            alias_ok(&action.workspace)?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "window", action.window.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "select-numbered" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            match action.index {
                Some(1..=9) => {}
                _ => return Err(fail(at, "index must be 1..=9")),
            }
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "window", action.window.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "select-relative" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            match action.delta {
                Some(-1) | Some(1) => {}
                _ => return Err(fail(at, "delta must be -1/+1")),
            }
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "window", action.window.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "select-previous" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "window", action.window.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "focus-directional" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            window_known(&action.window)?;
            match action.direction.as_deref() {
                Some("left") | Some("right") | Some("up") | Some("down") => {}
                _ => return Err(fail(at, "direction must be left/right/up/down")),
            }
            if action.expect_effect.is_none() || action.expect_target.is_none() {
                return Err(fail(at, "focus needs expect_effect/expect_target"));
            }
            window_known(&action.expect_target)?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(
                action,
                at,
                "checkpoint fields",
                checkpoint_view_fields(action),
            )?;
        }
        "move-directional" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            window_known(&action.window)?;
            match action.direction.as_deref() {
                Some("left") | Some("right") | Some("up") | Some("down") => {}
                _ => return Err(fail(at, "direction must be left/right/up/down")),
            }
            match action.same_axis.as_deref() {
                Some("swap-with-neighbor") | Some("group-with-neighbor") => {}
                _ => return Err(fail(at, "same_axis must name the neighbor mode")),
            }
            if action.expect_effect.is_none() || action.expect_target.is_none() {
                return Err(fail(at, "move needs expect_effect/expect_target"));
            }
            window_known(&action.expect_target)?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(
                action,
                at,
                "checkpoint fields",
                checkpoint_view_fields(action),
            )?;
        }
        "send" => {
            if action.seq.is_none() {
                return Err(fail(at, "command needs seq"));
            }
            window_known(&action.window)?;
            if action.output.is_none() {
                return Err(fail(at, "send needs an explicit output"));
            }
            alias_ok(&action.workspace)?;
            if action.follow.is_none() {
                return Err(fail(at, "send needs follow"));
            }
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "admit" => {
            window_known(&action.window)?;
            if action.rect.is_none() {
                return Err(fail(at, "admit needs rect"));
            }
            forbid(action, at, "seq", action.seq.is_some())?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "close" => {
            window_known(&action.window)?;
            forbid(action, at, "seq", action.seq.is_some())?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "focus-window" => {
            // Explicit foreground acquisition: the named live window must be
            // homed in the currently shown view. The native leg performs the
            // fixture focus with readback and aborts when acquisition fails;
            // this observation carries no command and never refocuses.
            window_known(&action.window)?;
            forbid(action, at, "seq", action.seq.is_some())?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "checkpoint fields", checkpoint_fields(action))?;
        }
        "checkpoint" => {
            if action.name.is_none()
                || action.view.is_none()
                || action.members.is_none()
                || action.homing.is_none()
                || action.focus.is_none()
                || action.foreground.is_none()
                || action.pager_current.is_none()
                || action.pager_occupied.is_none()
            {
                return Err(fail(
                    at,
                    "checkpoint needs name/view/members/homing/focus/foreground/pager",
                ));
            }
            if let Some(view) = &action.view
                && !is_alias(view)
            {
                return Err(fail(at, "checkpoint view must be home/away"));
            }
            if let Some(current) = &action.pager_current
                && !is_alias(current)
            {
                return Err(fail(at, "checkpoint pager_current must be home/away"));
            }
            forbid(action, at, "seq", action.seq.is_some())?;
            forbid(action, at, "output", action.output.is_some())?;
            forbid(action, at, "workspace", action.workspace.is_some())?;
            forbid(action, at, "index", action.index.is_some())?;
            forbid(action, at, "delta", action.delta.is_some())?;
            forbid(action, at, "window", action.window.is_some())?;
            forbid(action, at, "direction", action.direction.is_some())?;
            forbid(action, at, "same_axis", action.same_axis.is_some())?;
            forbid(action, at, "follow", action.follow.is_some())?;
            forbid(action, at, "rect", action.rect.is_some())?;
            forbid(action, at, "expect_effect", action.expect_effect.is_some())?;
            forbid(action, at, "expect_target", action.expect_target.is_some())?;
        }
        other => return Err(format!("{at}: command {other:?} is not covered")),
    }
    Ok(())
}

fn checkpoint_fields(action: &RawAction) -> bool {
    checkpoint_view_fields(action)
        || action.expect_effect.is_some()
        || action.expect_target.is_some()
}

fn checkpoint_view_fields(action: &RawAction) -> bool {
    action.name.is_some()
        || action.view.is_some()
        || action.members.is_some()
        || action.homing.is_some()
        || action.tiles.is_some()
        || action.focus.is_some()
        || action.foreground.is_some()
        || action.pager_current.is_some()
        || action.pager_occupied.is_some()
}

// ---------------------------------------------------------------------------
// Simulated host: report builder, pending proofs, schedules.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StressSchedule {
    Ordered,
    Batched,
    Reordered,
}

impl StressSchedule {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ordered => "ordered",
            Self::Batched => "batched",
            Self::Reordered => "reordered",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Ordered, Self::Batched, Self::Reordered]
    }
}

pub(crate) struct PendingMember {
    pub(crate) operation_id: String,
    pub(crate) target_output: OutputId,
    /// Logical workspace id resolved at command time: matches the retained
    /// transfer target exactly.
    pub(crate) target_workspace: String,
    pub(crate) pre: Option<WorkspaceRef>,
    pub(crate) attempt_ms: u64,
}

pub(crate) struct Pending {
    /// Latest view target (output, home/away alias) since the last receipt.
    /// Latest wins, exactly like core view retention. Aliases resolve to
    /// workspace ids when the receipt is built, so lifecycle slot rebinding
    /// between command and receipt stays consistent.
    pub(crate) view: Option<(String, String)>,
    /// Window homing proofs in command order.
    pub(crate) member_order: Vec<String>,
    pub(crate) members: BTreeMap<String, PendingMember>,
}

pub(crate) fn member_proof(window: &str, member: &PendingMember) -> MemberAttempt {
    MemberAttempt {
        operation_id: member.operation_id.clone(),
        window: WindowId(window.to_owned()),
        target_output: member.target_output.clone(),
        target_workspace: member.target_workspace.clone(),
        setter: FrameSetter::Accepted,
        pre: member.pre.clone(),
        post: Some(WorkspaceRef::Logical {
            output: member.target_output.clone(),
            workspace: WorkspaceId(member.target_workspace.clone()),
        }),
        attempt_ms: member.attempt_ms,
    }
}

impl Pending {
    pub(crate) fn empty() -> Self {
        Self {
            view: None,
            member_order: Vec::new(),
            members: BTreeMap::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.view.is_none() && self.members.is_empty()
    }
}

pub(crate) fn case_mode(mode: &str) -> Result<WorkspaceMode, String> {
    match mode {
        "per-output-local" => Ok(WorkspaceMode::PerOutputLocal),
        other => Err(format!("mode {other:?} is not covered")),
    }
}

pub(crate) fn build_command(
    action: &RawAction,
    binding: &BTreeMap<String, String>,
) -> Result<Option<WorldCommand>, String> {
    let seq_ok = |action: &RawAction| action.seq.ok_or("command needs seq".to_owned());
    match action.op.as_str() {
        "select-workspace" => {
            let _ = seq_ok(action)?;
            Ok(Some(WorldCommand::SelectWorkspace {
                output: OutputId(action.output.clone().expect("validated")),
                workspace: binding
                    .get(action.workspace.as_ref().expect("validated"))
                    .cloned()
                    .ok_or("unbound workspace alias".to_owned())?,
            }))
        }
        "select-numbered" => Ok(Some(WorldCommand::SelectNumbered {
            output: action.output.clone().map(OutputId),
            index: action.index.expect("validated"),
        })),
        "select-relative" => Ok(Some(WorldCommand::SelectRelative {
            output: action.output.clone().map(OutputId),
            delta: action.delta.expect("validated"),
        })),
        "select-previous" => Ok(Some(WorldCommand::SelectPrevious {
            output: action.output.clone().map(OutputId),
        })),
        "focus-directional" => Ok(Some(WorldCommand::FocusDirectional {
            window: WindowId(action.window.clone().expect("validated")),
            direction: action.direction.clone().expect("validated"),
        })),
        "move-directional" => {
            let same_axis = match action.same_axis.as_deref().expect("validated") {
                "swap-with-neighbor" => tiler_core::directional::SameAxisMove::SwapWithNeighbor,
                "group-with-neighbor" => tiler_core::directional::SameAxisMove::GroupWithNeighbor,
                _ => return Err("same_axis is not covered".to_owned()),
            };
            Ok(Some(WorldCommand::MoveDirectional {
                window: WindowId(action.window.clone().expect("validated")),
                direction: action.direction.clone().expect("validated"),
                same_axis,
            }))
        }
        "send" => Ok(Some(WorldCommand::Send {
            window: WindowId(action.window.clone().expect("validated")),
            target_output: OutputId(action.output.clone().expect("validated")),
            target_workspace: binding
                .get(action.workspace.as_ref().expect("validated"))
                .cloned()
                .ok_or("unbound workspace alias".to_owned())?,
            follow: action.follow.expect("validated"),
        })),
        "admit" | "close" | "checkpoint" => Ok(None),
        other => Err(format!("command {other:?} is not covered")),
    }
}

/// Scale authored tiles from the authored work area to a discovered one so
/// a later native runner can reuse the same oracle on a different panel.
/// Zero-gap halves must stay integral; anything else refuses.
pub fn normalize_tiles_to_work(
    tiles: &BTreeMap<String, [i32; 4]>,
    authored_work: [i32; 4],
    discovered_work: [i32; 4],
) -> Result<BTreeMap<String, [i32; 4]>, String> {
    if authored_work == discovered_work {
        return Ok(tiles.clone());
    }
    let (ax, ay, aw, ah) = (
        authored_work[0],
        authored_work[1],
        authored_work[2],
        authored_work[3],
    );
    let (dx, dy, dw, dh) = (
        discovered_work[0],
        discovered_work[1],
        discovered_work[2],
        discovered_work[3],
    );
    if aw <= 0 || ah <= 0 || dw <= 0 || dh <= 0 {
        return Err("work areas must have positive size".to_owned());
    }
    let mut out = BTreeMap::new();
    for (window, tile) in tiles {
        let scale = |v: i32, a0: i32, a: i32, d0: i32, d: i32| -> Result<i32, String> {
            let num = i64::from(v - a0) * i64::from(d);
            if num % i64::from(a) != 0 {
                return Err(format!("tile for {window} does not scale integrally"));
            }
            Ok((num / i64::from(a)) as i32 + d0)
            // `as` is exact here: the quotient fits i32 when both work
            // areas are carried geometry.
        };
        // Width/height scale from zero, origins shift by the work offset.
        let x = scale(tile[0], ax, aw, dx, dw)?;
        let y = scale(tile[1], ay, ah, dy, dh)?;
        let w = scale(tile[2], 0, aw, 0, dw)?;
        let h = scale(tile[3], 0, ah, 0, dh)?;
        out.insert(window.clone(), [x, y, w, h]);
    }
    coverage_ok(discovered_work, &out.values().copied().collect::<Vec<_>>())
        .map_err(|e| format!("normalized tiles do not tile: {e}"))?;
    Ok(out)
}

pub(crate) fn bind_engine(
    engine: &mut WorldEngine,
    case: &StressCase,
    mode: WorkspaceMode,
) -> Result<(), String> {
    use tiler_core::connection::{
        ApplicationIdentity, HandshakeRequest, HostCapabilities, RawSettingValue, SettingsRequest,
        SettingsStatus,
    };
    let handshake = engine.handshake(&HandshakeRequest {
        adapter: case.adapter.clone(),
        request: "stress-handshake".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: None,
            process_id: None,
        },
        capabilities: HostCapabilities {
            // Stress facts carry native sticky and maximize facts; only
            // native identity is unavailable on this lane.
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::without_native_identity()
        },
    });
    let generation = handshake.generation.ok_or("stress handshake failed")?;
    let mode = match mode {
        WorkspaceMode::PerOutputLocal => "per-output-local",
        WorkspaceMode::GlobalUnique => "global-unique",
        WorkspaceMode::Shared => "shared",
    };
    let settings = engine
        .handle_settings(&SettingsRequest {
            adapter: case.adapter.clone(),
            generation,
            request: "stress-settings".to_owned(),
            revision: 1,
            values: vec![
                ("mode".to_owned(), RawSettingValue::String(mode.to_owned())),
                ("gap".to_owned(), RawSettingValue::Number(0.0)),
                ("outer".to_owned(), RawSettingValue::Number(0.0)),
            ],
        })
        .ok_or("stress settings envelope rejected")?;
    if settings.status != SettingsStatus::Accepted {
        return Err("stress settings were not accepted".to_owned());
    }
    Ok(())
}

/// Converge simulated host facts to the actual accepted allocations, exactly
/// like a perfect host applying every desired frame. The oracle literals
/// stay hand-authored; this only keeps later facts readable.
pub(crate) fn resolve(
    binding: &BTreeMap<String, String>,
    alias: &str,
    at: &str,
) -> Result<String, String> {
    binding
        .get(alias)
        .cloned()
        .ok_or_else(|| fail(at, format!("unknown workspace alias {alias:?}")))
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
#[derive(Debug)]
pub struct StressStats {
    pub reports: usize,
    pub checkpoints: usize,
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    fn duo_path() -> String {
        format!(
            "{}/../../test-fixtures/world-stress-home-away.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn trio_path() -> String {
        format!(
            "{}/../../test-fixtures/world-stress-trio-churn.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn duo_value() -> serde_json::Value {
        let text = std::fs::read_to_string(duo_path()).expect("read duo fixture");
        serde_json::from_str(&text).expect("parse duo fixture")
    }

    fn load_value(value: &serde_json::Value) -> Result<StressCase, String> {
        let text = serde_json::to_string(value).expect("re-encode");
        let case: StressCase =
            serde_json::from_str(&text).map_err(|e| format!("parse case: {e}"))?;
        validate_case(&case)?;
        Ok(case)
    }

    fn action_of<'v>(value: &'v mut serde_json::Value, name: &str) -> &'v mut serde_json::Value {
        value["actions"]
            .as_array_mut()
            .expect("actions")
            .iter_mut()
            .find(|a| a["op"] == "checkpoint" && a["name"] == name)
            .unwrap_or_else(|| panic!("checkpoint {name}"))
    }

    #[test]
    fn unknown_field_refuses() {
        let mut value = duo_value();
        value["actions"][0]["bogus"] = serde_json::json!(1);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("parse case"), "unexpected: {error}");
    }

    #[test]
    fn unknown_op_refuses() {
        let mut value = duo_value();
        value["actions"][0]["op"] = serde_json::json!("warp");
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("not covered"), "unexpected: {error}");
    }

    #[test]
    fn missing_checkpoint_field_refuses() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")
            .as_object_mut()
            .expect("checkpoint")
            .remove("focus");
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("checkpoint needs"), "unexpected: {error}");
    }

    #[test]
    fn missing_foreground_refuses() {
        let mut value = duo_value();
        action_of(&mut value, "show-F1")
            .as_object_mut()
            .expect("checkpoint")
            .remove("foreground");
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("foreground"), "unexpected: {error}");
    }

    #[test]
    fn focus_window_unknown_window_refuses() {
        let mut value = duo_value();
        let actions = value["actions"].as_array_mut().expect("actions");
        let focus = actions
            .iter()
            .position(|a| a["op"] == "focus-window")
            .expect("focus-window");
        actions[focus]["window"] = serde_json::json!("win-z");
        let error = load_value(&value).expect_err("must reject");
        assert!(
            error.contains("unknown or missing window"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn non_increasing_seq_refuses() {
        let mut value = duo_value();
        let actions = value["actions"].as_array_mut().expect("actions");
        let first = actions
            .iter()
            .find(|a| a["seq"].is_number())
            .expect("seq")
            .clone();
        actions[2] = first;
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("strictly increasing"), "unexpected: {error}");
    }

    #[test]
    fn same_window_double_send_refuses() {
        let mut value = duo_value();
        let actions = value["actions"].as_array_mut().expect("actions");
        let send = actions
            .iter()
            .find(|a| a["op"] == "send")
            .expect("send")
            .clone();
        actions.insert(5, send);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("without a receipt"), "unexpected: {error}");
    }

    #[test]
    fn wrong_final_members_rejected() {
        let mut value = duo_value();
        value["final"]["members"] = serde_json::json!(["win-a"]);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("homing/tiles"), "unexpected: {error}");
    }

    #[test]
    fn checkpoint_members_must_match_live_roster() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")["members"] = serde_json::json!(["win-a"]);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("live roster"), "unexpected: {error}");
    }

    #[test]
    fn unknown_alias_refuses() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")["view"] = serde_json::json!("void");
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("home/away"), "unexpected: {error}");
    }

    #[test]
    fn cases_use_group_moves_only() {
        // The exact-owner `directional --move` control carries no same-axis
        // mode: the native leg only honors the product default. Cases must
        // not silently introduce swap-mode moves.
        for path in [duo_path(), trio_path()] {
            let text = std::fs::read_to_string(&path).expect("read case");
            let value: serde_json::Value = serde_json::from_str(&text).expect("parse case");
            for action in value["actions"].as_array().expect("actions") {
                if action["op"] == "move-directional" {
                    assert_eq!(
                        action["same_axis"],
                        serde_json::json!("group-with-neighbor"),
                        "non-group move in {}",
                        path
                    );
                }
            }
        }
    }
    #[test]
    fn coverage_rejects_bad_tilings() {
        let work = [0, 0, 800, 560];
        assert!(coverage_ok(work, &[[0, 0, 400, 560], [400, 0, 400, 560]]).is_ok());
        assert!(coverage_ok(work, &[[0, 0, 399, 560], [400, 0, 400, 560]]).is_err());
        assert!(coverage_ok(work, &[[0, 0, 400, 560], [400, 0, 400, 0]]).is_err());
        assert!(coverage_ok(work, &[[0, 0, 800, 560], [0, 0, 800, 560]]).is_err());
        assert!(coverage_ok(work, &[]).is_err());
    }

    #[test]
    fn normalize_tiles_identity_and_scale() {
        let work = [0, 0, 800, 560];
        let tiles = BTreeMap::from([
            ("win-a".to_owned(), [400, 0, 400, 560]),
            ("win-b".to_owned(), [0, 0, 400, 560]),
        ]);
        assert_eq!(
            normalize_tiles_to_work(&tiles, work, work).expect("identity"),
            tiles
        );
        let doubled = normalize_tiles_to_work(&tiles, work, [0, 0, 1600, 1120]).expect("scale");
        assert_eq!(doubled["win-a"], [800, 0, 800, 1120]);
        assert_eq!(doubled["win-b"], [0, 0, 800, 1120]);
        assert!(normalize_tiles_to_work(&tiles, work, [0, 0, 801, 560]).is_err());
    }
}
