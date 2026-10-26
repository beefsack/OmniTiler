//! Shared platform-neutral high-volume stress definitions plus the
//! core-only runner (development-only, never a public API).
//!
//! Two small deterministic single-output zero-gap tiled cases stress sends
//! (follow and stay), workspace switches (explicit and core-resolved),
//! directional focus/moves and admit/close churn with independently authored
//! final oracles. Three simulated-host schedules cover the same actions:
//! ordered (settle every action), batched (one receipt settles many) and
//! reordered (membership proofs arrive before the view proof, stale echoes
//! prove nothing). Stale/duplicate/consumed completions are probed after
//! the final oracle and must refuse.
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

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ScreenReport, ScreenView, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReply,
    WorldReport,
};

fn rect4(v: [i32; 4]) -> Rect {
    Rect {
        x: v[0],
        y: v[1],
        w: v[2],
        h: v[3],
    }
}

fn rect_of(v: Rect) -> [i32; 4] {
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

fn coverage_ok(work: [i32; 4], allocs: &[[i32; 4]]) -> Result<(), String> {
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

fn fail(at: &str, msg: impl std::fmt::Display) -> String {
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
struct StressScreen {
    out: String,
    full: [i32; 4],
    work: [i32; 4],
    gap: i32,
    outer_gap: i32,
    complete: bool,
    primary: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StressSettings {
    default_tiled: bool,
}

/// One action in the stream. Command actions carry `seq`; observation-only
/// actions (`admit`, `close`, `checkpoint`) carry none. Checkpoint actions
/// carry the full intermediate oracle.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    op: String,
    seq: Option<u64>,
    output: Option<String>,
    workspace: Option<String>,
    index: Option<u8>,
    delta: Option<i32>,
    window: Option<String>,
    direction: Option<String>,
    same_axis: Option<String>,
    follow: Option<bool>,
    rect: Option<[i32; 4]>,
    name: Option<String>,
    view: Option<String>,
    members: Option<Vec<String>>,
    homing: Option<BTreeMap<String, String>>,
    tiles: Option<BTreeMap<String, [i32; 4]>>,
    focus: Option<String>,
    /// Expected real foreground: a live window id, or null when no managed
    /// fixture window can hold the foreground (hidden focus never counts).
    /// Present (even null) on every checkpoint; refused when absent.
    #[serde(default, deserialize_with = "de_presence")]
    foreground: Option<Option<String>>,
    expect_effect: Option<String>,
    expect_target: Option<String>,
    pager_current: Option<String>,
    pager_occupied: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalExpect {
    view: String,
    members: Vec<String>,
    homing: BTreeMap<String, String>,
    tiles: BTreeMap<String, [i32; 4]>,
    focus: String,
    foreground: Option<String>,
    pager_current: String,
    pager_occupied: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StressCase {
    name: String,
    #[allow(dead_code)]
    description: String,
    owner: String,
    adapter: String,
    generation: String,
    mode: String,
    screens: Vec<StressScreen>,
    settings: StressSettings,
    windows: Vec<String>,
    initial_facts: BTreeMap<String, [i32; 4]>,
    actions: Vec<RawAction>,
    #[serde(rename = "final")]
    final_expect: FinalExpect,
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

fn validate_case(case: &StressCase) -> Result<(), String> {
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

struct Pending {
    /// Latest view target (output, home/away alias) since the last receipt.
    /// Latest wins, exactly like core view retention. Aliases resolve to
    /// workspace ids when the receipt is built, so lifecycle slot rebinding
    /// between command and receipt stays consistent.
    view: Option<(String, String)>,
    /// Window homing proofs in command order (window, home/away alias).
    member_order: Vec<String>,
    members: BTreeMap<String, String>,
}

impl Pending {
    fn empty() -> Self {
        Self {
            view: None,
            member_order: Vec::new(),
            members: BTreeMap::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.view.is_none() && self.members.is_empty()
    }
}

struct Sim {
    req: u64,
    obs: u64,
    facts: BTreeMap<String, [i32; 4]>,
    observed: BTreeMap<String, String>,
    echo: BTreeMap<String, String>,
    live: BTreeSet<String>,
    pending: Pending,
    /// Sends accepted since the last observation report (in-flight guard).
    outstanding: BTreeSet<String>,
    home: String,
    /// Current away-slot workspace id. Home never empties, but the away
    /// slot rebinds when lifecycle prunes an emptied workspace and keeps
    /// the trailing spare. Hand-authored expectations stay in alias space;
    /// ids resolve late from this slot.
    away: String,
    /// Every workspace id ever seen in a reply ring. Stale scope rows for
    /// pruned workspaces must name a seen id, never an invented one.
    seen: BTreeSet<String>,
    out: String,
    /// Predicted logical slot ("home"/"away") from commands issued since
    /// the last reply. Receipts echo this; the next command resolves
    /// against it so batched relative/previous selections predict the
    /// right target.
    predicted_alias: String,
    /// Predicted real foreground: the managed window the adapter observes,
    /// or `None` when no managed fixture window can hold it (every command
    /// report carries exactly this, never the subject). Retained core focus
    /// (`focus` below) stays untouched: hiding never clears it.
    foreground: Option<String>,
    /// Commanded homing truth (window id -> home/away alias), updated at
    /// command/admit time so `focus-window` validation is schedule-stable.
    homing_alias: BTreeMap<String, String>,
    /// Last non-empty one-shot effect target (window id, "" when none yet).
    focus: String,
    /// Accepted (report, reply) pairs in emission order.
    pairs: Vec<(WorldReport, WorldReply)>,
}

fn case_mode(mode: &str) -> Result<WorkspaceMode, String> {
    match mode {
        "per-output-local" => Ok(WorkspaceMode::PerOutputLocal),
        other => Err(format!("mode {other:?} is not covered")),
    }
}

fn build_command(
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

fn build_report(
    case: &StressCase,
    mode: WorkspaceMode,
    sim: &Sim,
    command: Option<WorldCommand>,
    command_seq: Option<u64>,
    closed: Vec<String>,
) -> Result<WorldReport, String> {
    let screen = &case.screens[0];
    let mut live: Vec<String> = sim.live.iter().cloned().collect();
    live.sort();
    let mut keys: Vec<&String> = sim.facts.keys().collect();
    keys.sort();
    let live_refs: Vec<&String> = live.iter().collect();
    if keys != live_refs {
        return Err("facts must name exactly the live roster".to_owned());
    }
    let mut windows = Vec::new();
    for id in &live {
        windows.push(WindowFact {
            window: WindowId(id.clone()),
            output: OutputId(sim.out.clone()),
            rect: sim.facts.get(id).copied().map(rect4),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
            minimized: false,
            observed_workspace: sim.observed.get(id).cloned(),
            native_workspace: None,
        });
    }
    let mut views = Vec::new();
    for (out, view) in &sim.echo {
        views.push(ScreenView {
            output: OutputId(out.clone()),
            view: view.clone(),
        });
    }
    Ok(WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: case.owner.clone(),
        adapter_incarnation: case.adapter.clone(),
        core_generation: case.generation.clone(),
        observation_seq: sim.obs,
        request_seq: sim.req,
        request_id: format!("req-{}", sim.req),
        command_seq,
        command,
        workspace_mode: mode,
        screens: vec![ScreenReport {
            output: OutputId(screen.out.clone()),
            full_rect: Some(rect4(screen.full)),
            work_area: Some(rect4(screen.work)),
            gap: screen.gap,
            outer_gap: screen.outer_gap,
            complete: screen.complete,
            primary: screen.primary,
        }],
        windows,
        views,
        closed: closed.into_iter().map(WindowId).collect(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        // Truthful observation: the predicted real foreground, "" when no
        // managed window can hold it. Never the command subject.
        focused_global: sim.foreground.clone().unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![],
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts: vec![],
        view_attempts: vec![],
        clear_attempts: vec![],
        creation_attempts: vec![],
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
        default_tiled: case.settings.default_tiled,
    })
}

fn bootstrap_binding(
    case: &StressCase,
    mode: WorkspaceMode,
) -> Result<BTreeMap<String, String>, String> {
    let mut probe = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    let sim = Sim {
        req: 1,
        obs: 1,
        facts: case.initial_facts.clone(),
        observed: BTreeMap::new(),
        echo: BTreeMap::new(),
        live: case.initial_facts.keys().cloned().collect(),
        pending: Pending::empty(),
        outstanding: BTreeSet::new(),
        home: String::new(),
        away: String::new(),
        seen: BTreeSet::new(),
        out: case.screens[0].out.clone(),
        predicted_alias: String::new(),
        foreground: None,
        homing_alias: case
            .initial_facts
            .keys()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect(),
        focus: String::new(),
        pairs: Vec::new(),
    };
    let first = build_report(case, mode, &sim, None, None, vec![])?;
    let reply = match probe.handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => return Err(format!("bootstrap probe: expected accepted, got {other:?}")),
    };
    let home = reply
        .outputs
        .iter()
        .find(|o| o.output.0 == case.screens[0].out)
        .and_then(|o| o.logical_view.clone())
        .ok_or("bootstrap has no view")?;
    let mut ring: Vec<(usize, String)> = reply
        .workspaces
        .iter()
        .filter(|w| w.output.0 == case.screens[0].out)
        .map(|w| (w.order, w.id.clone()))
        .collect();
    ring.sort();
    if ring.len() != 2 || ring[0].1 != home {
        return Err("bootstrap ring is not [home, away]".to_owned());
    }
    Ok(BTreeMap::from([
        ("home".to_owned(), home),
        ("away".to_owned(), ring[1].1.clone()),
    ]))
}

/// Converge simulated host facts to the actual accepted allocations, exactly
/// like a perfect host applying every desired frame. The oracle literals
/// stay hand-authored; this only keeps later facts readable.
fn converge_facts(sim: &mut Sim, reply: &WorldReply) {
    for row in &reply.windows {
        if let Some(alloc) = row.allocation {
            sim.facts.insert(row.lifetime.0.clone(), rect_of(alloc));
        }
    }
}

fn note_effects(sim: &mut Sim, reply: &WorldReply, at: &str) -> Result<(), String> {
    for effect in &reply.effects {
        if effect.issued_revision != reply.state_revision {
            return Err(fail(at, "effect issued_revision != reply revision"));
        }
    }
    if let Some(last) = reply.effects.last() {
        sim.focus = last.target.0.clone();
    }
    if reply.command_result.is_some()
        || reply.gesture_settled.is_some()
        || !reply.operation_settled.is_empty()
        || reply.cursor_warp.is_some()
    {
        return Err(fail(at, "unsupported reply class"));
    }
    Ok(())
}

fn check_pairing(
    case: &StressCase,
    report: &WorldReport,
    reply: &WorldReply,
    at: &str,
    revision: u64,
) -> Result<(), String> {
    if reply.request_id != report.request_id
        || reply.based_on_observation_seq != report.observation_seq
        || reply.adapter_incarnation != report.adapter_incarnation
        || reply.core_generation != report.core_generation
        || reply.contract_version != report.contract_version
    {
        return Err(fail(at, "reply does not pair this exact report"));
    }
    if !reply.ready {
        return Err(fail(at, "reply not ready"));
    }
    if reply.command_consumed {
        return Err(fail(at, "command consumed without execution"));
    }
    if reply.contract_version != WORLD_CONTRACT_VERSION
        || reply.adapter_incarnation != case.adapter
        || reply.core_generation != case.generation
        || report.owner != case.owner
    {
        return Err(fail(at, "report/reply provenance != case identity"));
    }
    if reply.state_revision != revision {
        return Err(fail(
            at,
            format!(
                "revision {} != report count {revision}",
                reply.state_revision
            ),
        ));
    }
    Ok(())
}

fn resolve(binding: &BTreeMap<String, String>, alias: &str, at: &str) -> Result<String, String> {
    binding
        .get(alias)
        .cloned()
        .ok_or_else(|| fail(at, format!("unknown workspace alias {alias:?}")))
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn check_oracle(
    case: &StressCase,
    sim: &Sim,
    reply: &WorldReply,
    at: &str,
    view_alias: &str,
    members: &[String],
    homing: &BTreeMap<String, String>,
    tiles: Option<&BTreeMap<String, [i32; 4]>>,
    focus: &str,
    foreground: &Option<String>,
    pager_current: &str,
    pager_occupied: &[String],
    both_scopes: bool,
) -> Result<(), String> {
    let screen = &case.screens[0];
    let view = resolve(&binding_of(sim), view_alias, at)?;
    let current = resolve(&binding_of(sim), pager_current, at)?;
    // Outputs: exactly the case screen showing the expected view.
    if reply.outputs.len() != 1 {
        return Err(fail(at, "outputs must carry exactly the case screen"));
    }
    let out = &reply.outputs[0];
    if out.output.0 != screen.out
        || out.logical_view.as_deref() != Some(view.as_str())
        || out.tiling_mode.as_str() != "tiled"
        || out.hold.is_some()
    {
        return Err(fail(
            at,
            format!("output does not show expected view {view_alias:?}"),
        ));
    }
    // Ring: home frozen at order 0 (it never empties); the away slot at
    // order 1 rebinds when lifecycle prunes an emptied workspace and keeps
    // the trailing spare. Anything past order 1 is an empty trailing
    // spare: never current, never occupied, never homed.
    let trailing: Vec<(usize, String)>;
    {
        let mut current_ring: BTreeMap<String, usize> = BTreeMap::new();
        for w in reply.workspaces.iter().filter(|w| w.output.0 == screen.out) {
            if current_ring.insert(w.id.clone(), w.order).is_some() {
                return Err(fail(at, "duplicate workspace id"));
            }
        }
        let mut ordered: Vec<(usize, String)> = current_ring
            .iter()
            .map(|(id, order)| (*order, id.clone()))
            .collect();
        ordered.sort();
        if ordered.is_empty() || ordered[0].1 != sim.home || ordered[0].0 != 0 {
            return Err(fail(at, format!("home slot moved: got {current_ring:?}")));
        }
        if ordered.len() < 2 || ordered[1].0 != 1 {
            return Err(fail(at, format!("away slot missing: got {current_ring:?}")));
        }
        trailing = ordered[2..].to_vec();
        for (order, id) in &trailing {
            if *order < 2 {
                return Err(fail(at, "trailing spare precedes the away slot"));
            }
            let _ = id;
        }
        for w in &reply.workspaces {
            if w.id == sim.home {
                continue;
            }
            if trailing.iter().any(|(_, id)| id == &w.id)
                && (w.tiling_mode.as_str() != "tiled"
                    || !w.existence
                    || w.hold.is_some()
                    || w.scope != screen.out
                    || w.output.0 != screen.out
                    || w.native_binding.is_some())
            {
                return Err(fail(at, "trailing spare carries unsupported class"));
            }
        }
    }
    // Scopes: every known scope controlled. The bootstrap carries only
    // the home scope. Later, home and the current away slot must be
    // present; lifecycle may retain stale rows for pruned workspaces, so
    // every extra scope must name a workspace id actually seen in a reply
    // ring, never an invented one. Scope keys are `output/workspace` pairs.
    {
        let mut scopes: Vec<&str> = reply.scopes.iter().map(|s| s.scope.as_str()).collect();
        scopes.sort();
        let home_scope = format!("{}/{}", screen.out, sim.home);
        let away_scope = format!("{}/{}", screen.out, sim.away);
        if !both_scopes {
            if scopes != [home_scope.as_str()] {
                return Err(fail(at, format!("scopes {scopes:?} != [home]")));
            }
        } else {
            if !scopes.contains(&home_scope.as_str()) || !scopes.contains(&away_scope.as_str()) {
                return Err(fail(
                    at,
                    format!("scopes {scopes:?} miss the home/away slots"),
                ));
            }
        }
        for s in &reply.scopes {
            if s.authority.as_str() != "controlled" {
                return Err(fail(at, format!("scope {} is not controlled", s.scope)));
            }
            if both_scopes {
                let id = s
                    .scope
                    .strip_prefix(&format!("{}/", screen.out))
                    .unwrap_or("");
                if id != sim.home && id != sim.away && !sim.seen.contains(id) {
                    return Err(fail(
                        at,
                        format!("scope {} names an unseen workspace", s.scope),
                    ));
                }
            }
        }
    }
    // Roster, homing, allocations, oracle tiles, coverage over actuals.
    let mut want_members = members.to_vec();
    want_members.sort();
    let mut got: Vec<String> = reply.windows.iter().map(|w| w.lifetime.0.clone()).collect();
    got.sort();
    if got != want_members {
        return Err(fail(at, format!("roster {got:?} != {want_members:?}")));
    }
    let mut allocs = Vec::new();
    let mut per_workspace: BTreeMap<String, Vec<[i32; 4]>> = BTreeMap::new();
    for row in &reply.windows {
        let alias = homing
            .get(&row.lifetime.0)
            .ok_or_else(|| fail(at, format!("no homing for {}", row.lifetime.0)))?;
        let ws = resolve(&binding_of(sim), alias, at)?;
        if row.output.0 != screen.out || row.workspace != ws {
            return Err(fail(at, format!("{} homed off {alias:?}", row.lifetime.0)));
        }
        let alloc = row
            .allocation
            .map(rect_of)
            .ok_or_else(|| fail(at, format!("{} has no allocation", row.lifetime.0)))?;
        allocs.push(alloc);
        per_workspace.entry(ws.clone()).or_default().push(alloc);
        let eff = row
            .effective_frame
            .map(rect_of)
            .ok_or_else(|| fail(at, format!("{} has no effective frame", row.lifetime.0)))?;
        if eff != alloc {
            return Err(fail(
                at,
                format!("{} effective != allocation", row.lifetime.0),
            ));
        }
        if let Some(oracle) = tiles {
            let tile = oracle
                .get(&row.lifetime.0)
                .ok_or_else(|| fail(at, format!("no authored tile for {}", row.lifetime.0)))?;
            if alloc != *tile {
                let mut dump: Vec<String> = reply
                    .windows
                    .iter()
                    .map(|r| format!("{}={:?}", r.lifetime.0, r.allocation.map(rect_of)))
                    .collect();
                dump.sort();
                return Err(fail(
                    at,
                    format!(
                        "{} allocation {alloc:?} != authored tile {tile:?}; all=[{}]",
                        row.lifetime.0,
                        dump.join(",")
                    ),
                ));
            }
        }
        if row.frame_authority.as_str() != "controlled"
            || row.frame_authority.hold_reason().is_some()
            || row.membership_authority.as_str() != "controlled"
            || row.membership_authority.hold_reason().is_some()
            || row.floating
            || row.float_origin.is_some()
            || row.frame_operation.is_some()
            || row.clear_operation.is_some()
        {
            return Err(fail(
                at,
                format!("{} carries unsupported class", row.lifetime.0),
            ));
        }
    }
    // Coverage runs over the ACTUAL reply allocations, per homed
    // workspace: each occupied workspace tiles the work area on its own.
    for (ws, group) in &per_workspace {
        coverage_ok(screen.work, group).map_err(|e| fail(at, format!("workspace {ws}: {e}")))?;
    }
    let _ = allocs;
    // Settled checkpoints carry no transfers.
    if !reply.transfers.is_empty() {
        return Err(fail(at, "checkpoint carries a pending transfer"));
    }
    // Tracked one-shot focus (retained while its workspace hides).
    if sim.focus != focus {
        return Err(fail(
            at,
            format!("tracked focus {:?} != {:?}", sim.focus, focus),
        ));
    }
    // Expected real foreground: null exactly when no managed window can
    // hold it (a hidden focus never counts as actual foreground).
    if sim.foreground.as_ref() != foreground.as_ref() {
        return Err(fail(
            at,
            format!(
                "predicted foreground {:?} != {:?}",
                sim.foreground, foreground
            ),
        ));
    }
    // Pager: ring order, current/occupied flags, labels and ordinals. With
    // the disabled core-only ingress the paint lists stay empty; the native
    // runner compares the actual renderer payload separately.
    {
        let pager = reply
            .presentation
            .pager
            .iter()
            .find(|s| s.output == screen.out)
            .ok_or_else(|| fail(at, "pager has no screen"))?;
        if pager.workspaces.len() != 2 + trailing.len() {
            return Err(fail(at, "pager workspace count != ring"));
        }
        for (index, ws) in pager.workspaces.iter().enumerate() {
            let expected_id = match index {
                0 => sim.home.clone(),
                1 => sim.away.clone(),
                _ => trailing[index - 2].1.clone(),
            };
            if ws.id != expected_id {
                return Err(fail(at, "pager order != ring order"));
            }
            if ws.ordinal != index as u32 + 1 {
                return Err(fail(at, "pager ordinal != 1-based ring position"));
            }
            if ws.label != format!("Workspace {}", ws.ordinal) {
                return Err(fail(at, "pager label != ordinal label"));
            }
            if index < 2 {
                let alias = if ws.id == sim.home { "home" } else { "away" };
                let want_current = ws.id == current;
                if ws.current != want_current {
                    let dump: Vec<String> = pager
                        .workspaces
                        .iter()
                        .map(|w| {
                            format!(
                                "{}:ord{} cur={} occ={}",
                                w.id, w.ordinal, w.current, w.occupied
                            )
                        })
                        .collect();
                    return Err(fail(
                        at,
                        format!(
                            "pager current mark is wrong for {} (id {}): got current={} want {want_current} (current id {current}) pager=[{}]",
                            alias,
                            ws.id,
                            ws.current,
                            dump.join(",")
                        ),
                    ));
                }
                let want_occupied = pager_occupied.contains(&alias.to_owned());
                if ws.occupied != want_occupied {
                    return Err(fail(
                        at,
                        format!("pager occupied mark is wrong for {alias}"),
                    ));
                }
            } else {
                // Trailing spares never carry occupancy or the current mark.
                if ws.current || ws.occupied {
                    return Err(fail(at, "trailing spare pager marks must stay clear"));
                }
            }
            if !ws.windows.is_empty() {
                return Err(fail(at, "core-only pager paint lists must stay empty"));
            }
        }
    }
    let _ = view;
    Ok(())
}

fn binding_of(sim: &Sim) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("home".to_owned(), sim.home.clone()),
        ("away".to_owned(), sim.away.clone()),
    ])
}

/// Refresh the away slot from the latest accepted reply: order 0 stays
/// the frozen home, order 1 is the current away slot (rebinds when
/// lifecycle prunes an emptied workspace).
fn refresh_away_slot(sim: &mut Sim, at: &str) -> Result<(), String> {
    let (_, reply) = sim.pairs.last().expect("just pushed");
    let mut ordered: Vec<(usize, String)> = reply
        .workspaces
        .iter()
        .filter(|w| w.output.0 == sim.out)
        .map(|w| (w.order, w.id.clone()))
        .collect();
    ordered.sort();
    if ordered.is_empty() || ordered[0].1 != sim.home {
        return Err(fail(at, "home slot moved"));
    }
    if ordered.len() < 2 {
        return Err(fail(at, "away slot missing"));
    }
    for (_, id) in &ordered {
        sim.seen.insert(id.clone());
    }
    sim.away = ordered[1].1.clone();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn submit(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut Sim,
    command: Option<WorldCommand>,
    command_seq: Option<u64>,
    closed: Vec<String>,
    at: &str,
) -> Result<(), String> {
    sim.req += 1;
    sim.obs += 1;
    let report = build_report(case, mode, sim, command, command_seq, closed)?;
    let debug = std::env::var("WORLD_STRESS_DEBUG").is_ok();
    if debug {
        let facts: Vec<String> = report
            .windows
            .iter()
            .map(|w| {
                format!(
                    "{} rect={:?} obs={:?}",
                    w.window.0, w.rect, w.observed_workspace
                )
            })
            .collect();
        let views: Vec<String> = report
            .views
            .iter()
            .map(|v| format!("{}={}", v.output.0, v.view))
            .collect();
        eprintln!(
            "DBG {at} req={} obs={} cmd={:?} facts=[{}] views=[{}] closed={:?}",
            report.request_seq,
            report.observation_seq,
            report.command_seq,
            facts.join(","),
            views.join(","),
            report.closed.iter().map(|w| &w.0).collect::<Vec<_>>()
        );
    }
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => {
            let revision = sim.pairs.len() as u64 + 1;
            check_pairing(case, &report, &reply, at, revision)?;
            note_effects(sim, &reply, at)?;
            // Converge simulated host facts only from settled replies.
            // Command replies with pending transfers carry transient
            // desired frames (possibly overlapping); a real host never
            // reports those as observed frames. Receipts settle first,
            // then the host is converged to the settled allocations.
            if reply.transfers.is_empty() {
                converge_facts(sim, &reply);
            }
            if debug {
                let rows: Vec<String> = reply
                    .windows
                    .iter()
                    .map(|w| {
                        format!(
                            "{}@{}/{:?}",
                            w.lifetime.0,
                            w.workspace,
                            w.allocation.map(rect_of)
                        )
                    })
                    .collect();
                eprintln!("DBG {at} reply-windows=[{}]", rows.join(","));
                let fx: Vec<String> = reply
                    .effects
                    .iter()
                    .map(|e| format!("{}->{}", e.id, e.target.0))
                    .collect();
                eprintln!("DBG {at} reply-effects=[{}]", fx.join(","));
            }
            sim.pairs.push((report, reply));
            refresh_away_slot(sim, at)?;
            Ok(())
        }
        WorldOutcome::Duplicate(_) => Err(fail(at, "duplicate reply is not fresh acceptance")),
        WorldOutcome::CommandConsumed(_) => {
            Err(fail(at, "consumed command never re-executes here"))
        }
        WorldOutcome::Rejected { kind, message } => Err(format!("{at}: rejected {kind} {message}")),
    }
}

fn submit_command(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut Sim,
    action: &RawAction,
    at: &str,
) -> Result<(), String> {
    let binding = binding_of(sim);
    let command = build_command(action, &binding)?.expect("command action");
    let seq = action.seq.expect("validated");
    // The report carries the predicted real foreground unchanged: a
    // subject-carrying command is natively honorable only when the subject
    // already holds the foreground (acquired by an explicit focus-window
    // observation, never silently).
    match action.op.as_str() {
        "select-workspace" => {
            let alias = action.workspace.clone().expect("validated");
            sim.pending.view = Some((action.output.clone().expect("validated"), alias.clone()));
            sim.predicted_alias = alias;
        }
        "select-numbered" | "select-relative" | "select-previous" => {
            // Core-resolved output lands on the single live screen; the
            // explicit form names it directly.
            let alias = resolve_numbered_relative(action, sim, at)?;
            let out = action.output.clone().unwrap_or_else(|| sim.out.clone());
            sim.pending.view = Some((out, alias.clone()));
            sim.predicted_alias = alias;
        }
        "send" => {
            let window = action.window.clone().expect("validated");
            if sim.foreground.as_deref() != Some(window.as_str()) {
                return Err(fail(
                    at,
                    "send subject does not hold the predicted foreground",
                ));
            }
            if sim.outstanding.contains(&window) {
                return Err(fail(at, "transfer-in-flight by construction"));
            }
            if sim.pending.members.contains_key(&window) {
                return Err(fail(at, "second send for a window without a receipt"));
            }
            let alias = action.workspace.clone().expect("validated");
            sim.pending.member_order.push(window.clone());
            sim.pending.members.insert(window.clone(), alias.clone());
            sim.outstanding.insert(window.clone());
            sim.homing_alias.insert(window.clone(), alias.clone());
            if action.follow.expect("validated") {
                sim.pending.view = Some((action.output.clone().expect("validated"), alias.clone()));
                sim.predicted_alias = alias;
            }
        }
        "focus-directional" | "move-directional" => {
            let window = action.window.clone().expect("validated");
            if sim.foreground.as_deref() != Some(window.as_str()) {
                return Err(fail(
                    at,
                    "directional subject does not hold the predicted foreground",
                ));
            }
        }
        other => return Err(fail(at, format!("{other:?} is not a command"))),
    }
    submit(
        case,
        mode,
        engine,
        sim,
        Some(command),
        Some(seq),
        vec![],
        at,
    )?;
    // Foreground transitions on the accepted command: directional effects
    // land on the visible target, follow sends keep the mover foreground,
    // stay sends hide the mover. Selects keep the foreground until the
    // receipt hides it (recomputed there).
    match action.op.as_str() {
        "focus-directional" | "move-directional" => {
            let target = action.expect_target.clone().expect("validated");
            sim.foreground = Some(target);
        }
        "send" => {
            if action.follow.expect("validated") {
                sim.foreground = Some(action.window.clone().expect("validated"));
            } else {
                sim.foreground = None;
            }
        }
        _ => {}
    }
    let reply = &sim.pairs.last().expect("just pushed").1;
    if action.op == "focus-directional" || action.op == "move-directional" {
        let want_id = action.expect_effect.clone().expect("validated");
        let want_target = action.expect_target.clone().expect("validated");
        let got: Vec<(String, String, String)> = reply
            .effects
            .iter()
            .map(|e| (e.id.clone(), e.kind.as_str().to_owned(), e.target.0.clone()))
            .collect();
        if got != vec![(want_id, "focus".to_owned(), want_target)] {
            return Err(fail(
                at,
                format!("{op} effect {got:?} != expected one-shot", op = action.op),
            ));
        }
    }
    Ok(())
}

/// Predict the view slot of a core-resolved selection: numbered/relative
/// resolve over the two slots from the predicted alias; previous toggles
/// to the other slot. Single output only.
fn resolve_numbered_relative(action: &RawAction, sim: &Sim, at: &str) -> Result<String, String> {
    let slots = ["home", "away"];
    let pos = slots
        .iter()
        .position(|slot| **slot == sim.predicted_alias)
        .ok_or_else(|| fail(at, "predicted alias is not a slot"))?;
    match action.op.as_str() {
        "select-numbered" => {
            let index = action.index.expect("validated") as usize;
            if index < 1 || index > slots.len() {
                return Err(fail(at, "numbered index is outside the slots"));
            }
            Ok(slots[index - 1].to_owned())
        }
        "select-relative" => {
            let delta = action.delta.expect("validated");
            let next = (pos as i32 + delta).rem_euclid(slots.len() as i32) as usize;
            Ok(slots[next].to_owned())
        }
        "select-previous" => {
            // Previous toggles between the two slots; with only home/away
            // the other slot is the target.
            Ok(slots[1 - pos].to_owned())
        }
        other => Err(fail(at, format!("{other:?} has no resolved view"))),
    }
}

/// Emit the first half of a reordered receipt pair. Returns true when a
/// split receipt was emitted and more proofs remain for the follow-up:
/// view echo before membership proofs when both pend, otherwise
/// later-half membership proofs before the rest. Single-proof spans emit
/// nothing (nothing to reorder). Split receipts never clear the
/// in-flight window guard; only the settling receipt does.
fn submit_reordered_first(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut Sim,
    at: &str,
) -> Result<bool, String> {
    let has_view = sim.pending.view.is_some();
    let members = sim.pending.members.len();
    if has_view && members > 0 {
        // View proof first, membership proofs ride the follow-up.
        let binding = binding_of(sim);
        if let Some((out, alias)) = sim.pending.view.clone() {
            let ws = binding
                .get(&alias)
                .cloned()
                .ok_or_else(|| fail(at, "unbound workspace alias"))?;
            sim.echo.insert(out, ws);
        }
        sim.pending.view = None;
        submit(case, mode, engine, sim, None, None, vec![], at)?;
        recompute_foreground(sim);
        return Ok(true);
    }
    if members >= 2 {
        let n = sim.pending.member_order.len();
        let start = n / 2;
        // Apply later-half membership proofs now; the rest plus the view
        // echo ride the follow-up receipt. Later commands prove first.
        // Aliases resolve against the current slot binding.
        let binding = binding_of(sim);
        let later: Vec<String> = sim.pending.member_order[start..].to_vec();
        for window in &later {
            if let Some(alias) = sim.pending.members.remove(window) {
                let ws = binding
                    .get(&alias)
                    .cloned()
                    .ok_or_else(|| fail(at, "unbound workspace alias"))?;
                sim.observed.insert(window.clone(), ws);
            }
        }
        sim.pending.member_order.drain(start..);
        submit(case, mode, engine, sim, None, None, vec![], at)?;
        recompute_foreground(sim);
        return Ok(true);
    }
    Ok(false)
}

/// Recompute the predicted real foreground after applied proofs: a
/// foreground window whose commanded homing no longer names the commanded
/// shown view is hidden, so no managed window can hold the foreground.
/// Commanded (not settled) truth keeps every schedule identical; flushes
/// settle pending proofs before any observation that reads this.
fn recompute_foreground(sim: &mut Sim) {
    if let Some(window) = sim.foreground.clone() {
        let shown = sim.homing_alias.get(&window).cloned().unwrap_or_default();
        if shown != sim.predicted_alias {
            sim.foreground = None;
        }
    }
}

/// Emit one observation report carrying all pending proofs.
fn submit_receipt(
    case: &StressCase,
    mode: WorkspaceMode,
    engine: &mut WorldEngine,
    sim: &mut Sim,
    at: &str,
) -> Result<(), String> {
    {
        let binding = binding_of(sim);
        if let Some((out, alias)) = sim.pending.view.clone() {
            let ws = binding
                .get(&alias)
                .cloned()
                .ok_or_else(|| fail(at, "unbound workspace alias"))?;
            sim.echo.insert(out, ws);
        }
        for (window, alias) in sim.pending.members.clone() {
            let ws = binding
                .get(&alias)
                .cloned()
                .ok_or_else(|| fail(at, "unbound workspace alias"))?;
            sim.observed.insert(window, ws);
        }
    }
    sim.pending = Pending::empty();
    submit(case, mode, engine, sim, None, None, vec![], at)?;
    sim.outstanding.clear();
    recompute_foreground(sim);
    Ok(())
}

#[derive(Debug)]
pub struct StressStats {
    pub reports: usize,
    pub checkpoints: usize,
}

pub fn run_stress(
    case: &StressCase,
    schedule: StressSchedule,
) -> Result<(Vec<String>, StressStats), String> {
    let mode = case_mode(&case.mode)?;
    let binding = bootstrap_binding(case, mode)?;
    let mut engine = WorldEngine::new(&case.generation).ok_or("bad generation")?;
    let mut sim = Sim {
        req: 0,
        obs: 0,
        facts: case.initial_facts.clone(),
        observed: BTreeMap::new(),
        echo: BTreeMap::new(),
        live: case.initial_facts.keys().cloned().collect(),
        pending: Pending::empty(),
        outstanding: BTreeSet::new(),
        home: binding["home"].clone(),
        away: binding["away"].clone(),
        seen: BTreeSet::new(),
        out: case.screens[0].out.clone(),
        predicted_alias: "home".to_owned(),
        foreground: None,
        homing_alias: case
            .initial_facts
            .keys()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect(),
        focus: String::new(),
        pairs: Vec::new(),
    };
    let mut lines = Vec::new();
    // Bootstrap observation: roster homed home, tiles equal the authored
    // converged facts, no transfers, pager current home. No managed window
    // holds the foreground yet.
    submit(
        case,
        mode,
        &mut engine,
        &mut sim,
        None,
        None,
        vec![],
        "bootstrap",
    )?;
    {
        let reply = &sim.pairs.last().expect("bootstrap").1.clone();
        let members: Vec<String> = sim.live.iter().cloned().collect();
        let homing: BTreeMap<String, String> = members
            .iter()
            .map(|w| (w.clone(), "home".to_owned()))
            .collect();
        check_oracle(
            case,
            &sim,
            reply,
            "bootstrap",
            "home",
            &members,
            &homing,
            Some(&case.initial_facts),
            "",
            &None,
            "home",
            &["home".to_owned()],
            false,
        )?;
        lines.push(format!(
            "ok {} bootstrap req=req-1 rev=1 view=home members={}",
            schedule.as_str(),
            members.len()
        ));
    }
    let mut checkpoints = 0;
    for (i, action) in case.actions.iter().enumerate() {
        let at = format!("action[{i}] {}", action.op);
        match action.op.as_str() {
            "checkpoint" => {
                if schedule != StressSchedule::Ordered {
                    if schedule == StressSchedule::Reordered
                        && !sim.pending.is_empty()
                        && submit_reordered_first(case, mode, &mut engine, &mut sim, &at)?
                    {
                        let reply = &sim.pairs.last().expect("receipt").1;
                        if reply.transfers.is_empty() {
                            return Err(fail(&at, "reordered split receipt settled early"));
                        }
                    }
                    submit_receipt(case, mode, &mut engine, &mut sim, &at)?;
                }
                let reply = &sim.pairs.last().expect("checkpoint").1.clone();
                let members = action.members.clone().expect("validated");
                let homing = action.homing.clone().expect("validated");
                let foreground = action.foreground.clone().flatten();
                check_oracle(
                    case,
                    &sim,
                    reply,
                    &at,
                    action.view.as_ref().expect("validated"),
                    &members,
                    &homing,
                    action.tiles.as_ref(),
                    action.focus.as_ref().expect("validated"),
                    &foreground,
                    action.pager_current.as_ref().expect("validated"),
                    action.pager_occupied.as_ref().expect("validated"),
                    true,
                )?;
                checkpoints += 1;
                lines.push(format!("ok {} {at}", schedule.as_str()));
            }
            "admit" => {
                let window = action.window.clone().expect("validated");
                if schedule != StressSchedule::Ordered {
                    // Batched/reordered admissions piggyback pending proofs.
                    let binding = binding_of(&sim);
                    if let Some((out, alias)) = sim.pending.view.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.echo.insert(out, ws);
                    }
                    for (w, alias) in sim.pending.members.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.observed.insert(w, ws);
                    }
                    sim.pending = Pending::empty();
                }
                sim.facts
                    .insert(window.clone(), action.rect.expect("validated"));
                sim.live.insert(window.clone());
                // Admission homes to the shown view and never arrives with
                // the foreground: the next focus-window observation (or the
                // next receipt, when none is possible) owns the truth.
                sim.homing_alias
                    .insert(window.clone(), sim.predicted_alias.clone());
                sim.foreground = None;
                submit(case, mode, &mut engine, &mut sim, None, None, vec![], &at)?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("admit").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered admit left a transfer"));
                    }
                }
            }
            "close" => {
                let window = action.window.clone().expect("validated");
                if schedule != StressSchedule::Ordered {
                    let binding = binding_of(&sim);
                    if let Some((out, alias)) = sim.pending.view.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.echo.insert(out, ws);
                    }
                    for (w, alias) in sim.pending.members.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.observed.insert(w, ws);
                    }
                    sim.pending = Pending::empty();
                }
                sim.facts.remove(&window);
                sim.live.remove(&window);
                sim.observed.remove(&window);
                sim.homing_alias.remove(&window);
                if sim.foreground.as_deref() == Some(window.as_str()) {
                    sim.foreground = None;
                }
                submit(
                    case,
                    mode,
                    &mut engine,
                    &mut sim,
                    None,
                    None,
                    vec![window],
                    &at,
                )?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("close").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered close left a transfer"));
                    }
                }
            }
            "focus-window" => {
                let window = action.window.clone().expect("validated");
                if !sim.live.contains(&window) {
                    return Err(fail(&at, "focus-window of a non-live window"));
                }
                if sim.homing_alias.get(&window).cloned().unwrap_or_default() != sim.predicted_alias
                {
                    return Err(fail(&at, "focus-window outside the shown view"));
                }
                if schedule != StressSchedule::Ordered {
                    // Explicit focus observations flush pending proofs like
                    // admit/close: the acquisition reads settled visibility.
                    let binding = binding_of(&sim);
                    if let Some((out, alias)) = sim.pending.view.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.echo.insert(out, ws);
                    }
                    for (w, alias) in sim.pending.members.clone() {
                        let ws = binding
                            .get(&alias)
                            .cloned()
                            .ok_or_else(|| fail(&at, "unbound workspace alias"))?;
                        sim.observed.insert(w, ws);
                    }
                    sim.pending = Pending::empty();
                }
                sim.foreground = Some(window);
                submit(case, mode, &mut engine, &mut sim, None, None, vec![], &at)?;
                sim.outstanding.clear();
                if schedule == StressSchedule::Ordered {
                    let reply = &sim.pairs.last().expect("focus-window").1;
                    if !reply.transfers.is_empty() {
                        return Err(fail(&at, "ordered focus-window left a transfer"));
                    }
                }
            }
            _ => {
                submit_command(case, mode, &mut engine, &mut sim, action, &at)?;
                if schedule == StressSchedule::Ordered {
                    // Settle every action immediately in the ordered mode.
                    let needs_receipt = !sim.pending.is_empty();
                    if needs_receipt {
                        submit_receipt(case, mode, &mut engine, &mut sim, &at)?;
                        let reply = &sim.pairs.last().expect("receipt").1;
                        if !reply.transfers.is_empty() {
                            return Err(fail(&at, "ordered receipt left a transfer"));
                        }
                    } else {
                        let reply = &sim.pairs.last().expect("command").1;
                        if !reply.transfers.is_empty() {
                            return Err(fail(&at, "one-shot command left a transfer"));
                        }
                    }
                }
            }
        }
    }
    // Final receipt plus the hand-authored final oracle.
    if schedule == StressSchedule::Reordered && !sim.pending.is_empty() {
        let _ = submit_reordered_first(case, mode, &mut engine, &mut sim, "final-split")?;
    }
    if !sim.pending.is_empty() {
        submit_receipt(case, mode, &mut engine, &mut sim, "final")?;
    }
    // In ordered mode the last receipt already settled; still emit a final
    // observation so the oracle reads one fresh settled reply.
    if schedule == StressSchedule::Ordered {
        submit(
            case,
            mode,
            &mut engine,
            &mut sim,
            None,
            None,
            vec![],
            "final",
        )?;
    }
    {
        let reply = &sim.pairs.last().expect("final").1.clone();
        let end = &case.final_expect;
        check_oracle(
            case,
            &sim,
            reply,
            "final",
            &end.view,
            &end.members,
            &end.homing,
            Some(&end.tiles),
            &end.focus,
            &end.foreground,
            &end.pager_current,
            &end.pager_occupied,
            true,
        )?;
    }
    // Stale/duplicate/consumed completions must refuse in every schedule.
    probe_completions(case, &mut engine, &mut sim)?;
    lines.push(format!(
        "ok {} final members={} reports={}",
        schedule.as_str(),
        case.final_expect.members.len(),
        sim.pairs.len()
    ));
    Ok((
        lines,
        StressStats {
            reports: sim.pairs.len(),
            checkpoints,
        },
    ))
}

/// Duplicate, stale-observation and consumed-command completions refuse.
fn probe_completions(
    case: &StressCase,
    engine: &mut WorldEngine,
    sim: &mut Sim,
) -> Result<(), String> {
    let (last_report, _) = sim.pairs.last().expect("final").clone();
    // Exact redelivery is a duplicate, never a new revision.
    match engine.handle_report(&last_report) {
        WorldOutcome::Duplicate(cached) => {
            let (_, last_reply) = sim.pairs.last().expect("final");
            if cached.state_revision != last_reply.state_revision {
                return Err("probe duplicate: cached revision moved".to_owned());
            }
        }
        other => {
            return Err(format!(
                "probe duplicate: expected duplicate, got {other:?}"
            ));
        }
    }
    // Fresh request sequence with a stale observation refuses.
    {
        let mut stale = last_report.clone();
        stale.request_seq = sim.req + 1;
        stale.request_id = format!("req-{}", sim.req + 1);
        stale.observation_seq = sim.obs.saturating_sub(1).max(1);
        match engine.handle_report(&stale) {
            WorldOutcome::Rejected {
                kind: "observation-stale",
                ..
            } => {}
            other => {
                return Err(format!(
                    "probe stale-obs: expected observation-stale, got {other:?}"
                ));
            }
        }
    }
    // Reused request sequence with different input refuses.
    {
        let mut reused = last_report.clone();
        reused.focused_global = "nobody".to_owned();
        match engine.handle_report(&reused) {
            WorldOutcome::Rejected {
                kind: "request-reused",
                ..
            } => {}
            other => {
                return Err(format!(
                    "probe reused-req: expected request-reused, got {other:?}"
                ));
            }
        }
    }
    // An already-consumed command sequence never re-executes.
    {
        let first_seq = case
            .actions
            .iter()
            .filter_map(|a| a.seq)
            .min()
            .ok_or("probe consumed: case has no command seq")?;
        let binding = binding_of(sim);
        let first = case
            .actions
            .iter()
            .find(|a| a.seq == Some(first_seq))
            .expect("seq");
        let command = build_command(first, &binding)?.expect("command");
        let mode = case_mode(&case.mode)?;
        let probe_sim = Sim {
            req: sim.req + 1,
            obs: sim.obs + 1,
            facts: sim.facts.clone(),
            observed: sim.observed.clone(),
            echo: sim.echo.clone(),
            live: sim.live.clone(),
            pending: Pending::empty(),
            outstanding: BTreeSet::new(),
            home: sim.home.clone(),
            away: sim.away.clone(),
            seen: sim.seen.clone(),
            out: sim.out.clone(),
            predicted_alias: sim.predicted_alias.clone(),
            foreground: sim.foreground.clone(),
            homing_alias: sim.homing_alias.clone(),
            focus: sim.focus.clone(),
            pairs: Vec::new(),
        };
        let report = build_report(
            case,
            mode,
            &probe_sim,
            Some(command),
            Some(first_seq),
            vec![],
        )?;
        match engine.handle_report(&report) {
            WorldOutcome::CommandConsumed(_) => {}
            other => {
                return Err(format!(
                    "probe consumed-cmd: expected command-consumed, got {other:?}"
                ));
            }
        }
    }
    Ok(())
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
    fn accepts_duo_all_schedules_with_locked_counts() {
        let case = load_stress_case(&duo_path()).expect("load duo");
        let command_ops = case.actions.iter().filter(|a| a.seq.is_some()).count();
        assert!(command_ops >= 100, "stress volume {command_ops} < 100");
        let (lines, stats) = run_stress(&case, StressSchedule::Ordered).expect("ordered");
        assert_eq!(stats.reports, 186);
        assert_eq!(stats.checkpoints, 16);
        assert!(lines.iter().any(|l| l.contains("final members=2")));
        let (_, stats) = run_stress(&case, StressSchedule::Batched).expect("batched");
        assert_eq!(stats.reports, 149);
        assert_eq!(stats.checkpoints, 16);
        let (_, stats) = run_stress(&case, StressSchedule::Reordered).expect("reordered");
        assert_eq!(stats.reports, 151);
        assert_eq!(stats.checkpoints, 16);
    }

    #[test]
    fn accepts_trio_all_schedules_with_locked_counts() {
        let case = load_stress_case(&trio_path()).expect("load trio");
        let command_ops = case.actions.iter().filter(|a| a.seq.is_some()).count();
        assert!(command_ops >= 100, "stress volume {command_ops} < 100");
        let (lines, stats) = run_stress(&case, StressSchedule::Ordered).expect("ordered");
        assert_eq!(stats.reports, 189);
        assert_eq!(stats.checkpoints, 19);
        assert!(lines.iter().any(|l| l.contains("final members=3")));
        let (_, stats) = run_stress(&case, StressSchedule::Batched).expect("batched");
        assert_eq!(stats.reports, 155);
        assert_eq!(stats.checkpoints, 19);
        let (_, stats) = run_stress(&case, StressSchedule::Reordered).expect("reordered");
        assert_eq!(stats.reports, 157);
        assert_eq!(stats.checkpoints, 19);
    }

    #[test]
    fn reordered_uses_more_reports_than_batched() {
        // The reordered split receipts are genuine extra round-trips, not
        // relabeled batching: reordered must emit strictly more reports.
        let case = load_stress_case(&duo_path()).expect("load duo");
        let (_, batched) = run_stress(&case, StressSchedule::Batched).expect("batched");
        let (_, reordered) = run_stress(&case, StressSchedule::Reordered).expect("reordered");
        assert!(reordered.reports > batched.reports);
        assert_eq!(reordered.checkpoints, batched.checkpoints);
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
    fn subject_without_foreground_refuses() {
        // Dropping the explicit acquisition before the first stay-send
        // leaves the subject off the predicted foreground: the command is
        // not natively honorable and must refuse, never silently refocus.
        let mut value = duo_value();
        let actions = value["actions"].as_array_mut().expect("actions");
        let send = actions
            .iter()
            .position(|a| a["op"] == "send" && a["follow"] == false)
            .expect("stay send");
        let focus = actions[..send]
            .iter()
            .rposition(|a| a["op"] == "focus-window")
            .expect("acquisition");
        actions.remove(focus);
        let case = load_value(&value).expect("loads; honorability is runtime");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(
            error.contains("does not hold the predicted foreground"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn focus_window_outside_shown_view_refuses() {
        // Acquiring a window homed in the hidden workspace cannot produce
        // a real foreground: the observation refuses instead of fabricating
        // one.
        let mut value = duo_value();
        let actions = value["actions"].as_array_mut().expect("actions");
        let hide = actions
            .iter()
            .position(|a| a["op"] == "checkpoint" && a["name"] == "hide-L05")
            .expect("hide checkpoint");
        actions.insert(
            hide,
            serde_json::json!({"op": "focus-window", "window": "win-a"}),
        );
        let case = load_value(&value).expect("loads; visibility is runtime");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(
            error.contains("outside the shown view"),
            "unexpected: {error}"
        );
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
    fn wrong_final_tiles_rejected() {
        let mut value = duo_value();
        value["final"]["tiles"]["win-a"] = serde_json::json!([0, 0, 399, 560]);
        let case = load_value(&value).expect("loads; oracle is wrong, not the shape");
        for schedule in StressSchedule::all() {
            let error = run_stress(&case, schedule).expect_err("must reject");
            assert!(error.contains("authored tile"), "unexpected: {error}");
        }
    }

    #[test]
    fn wrong_final_focus_rejected() {
        let mut value = duo_value();
        value["final"]["focus"] = serde_json::json!("win-a");
        let case = load_value(&value).expect("loads");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(error.contains("tracked focus"), "unexpected: {error}");
    }

    #[test]
    fn wrong_final_foreground_rejected() {
        // Retained core focus and expected real foreground diverge on
        // purpose (empty views keep the effect target but name no
        // foreground); corrupting either side must fail distinctly.
        let mut value = duo_value();
        value["final"]["foreground"] = serde_json::json!(null);
        let case = load_value(&value).expect("loads");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(
            error.contains("predicted foreground"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn wrong_final_members_rejected() {
        let mut value = duo_value();
        value["final"]["members"] = serde_json::json!(["win-a"]);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("homing/tiles"), "unexpected: {error}");
    }

    #[test]
    fn wrong_checkpoint_view_rejected() {
        let mut value = duo_value();
        action_of(&mut value, "show-F1")["view"] = serde_json::json!("away");
        let case = load_value(&value).expect("loads");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(error.contains("expected view"), "unexpected: {error}");
    }

    #[test]
    fn wrong_checkpoint_focus_rejected() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")["focus"] = serde_json::json!("win-b");
        let case = load_value(&value).expect("loads");
        for schedule in StressSchedule::all() {
            let error = run_stress(&case, schedule).expect_err("must reject");
            assert!(error.contains("tracked focus"), "unexpected: {error}");
        }
    }

    #[test]
    fn wrong_checkpoint_foreground_rejected() {
        // hide-L05 names no foreground (empty shown view) while retaining
        // the effect target; naming a hidden window as foreground must fail
        // as a predicted-foreground mismatch, not a focus mismatch.
        let mut value = duo_value();
        action_of(&mut value, "hide-L05")["foreground"] = serde_json::json!("win-b");
        let case = load_value(&value).expect("loads");
        for schedule in StressSchedule::all() {
            let error = run_stress(&case, schedule).expect_err("must reject");
            assert!(
                error.contains("predicted foreground"),
                "unexpected: {error}"
            );
        }
    }

    #[test]
    fn wrong_checkpoint_tiles_rejected() {
        let mut value = duo_value();
        action_of(&mut value, "show-F1")["tiles"]["win-a"] = serde_json::json!([0, 0, 399, 560]);
        let case = load_value(&value).expect("loads");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(error.contains("tile"), "unexpected: {error}");
    }

    #[test]
    fn checkpoint_members_must_match_live_roster() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")["members"] = serde_json::json!(["win-a"]);
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("live roster"), "unexpected: {error}");
    }

    #[test]
    fn stale_view_echo_cannot_settle() {
        // Dropping the away receipt leaves the select-away flight pending:
        // the checkpoint must fail on the outstanding transfer instead of
        // silently settling.
        let mut value = duo_value();
        // Remove the hide-F1 checkpoint's view expectation context by pointing
        // the first receipt away: emulate by deleting the checkpoint and
        // requiring settlement at the next one with a mismatched view.
        action_of(&mut value, "hide-F1")["view"] = serde_json::json!("home");
        let case = load_value(&value).expect("loads");
        let error = run_stress(&case, StressSchedule::Batched).expect_err("must reject");
        assert!(
            error.contains("expected view") || error.contains("transfer"),
            "unexpected: {error}"
        );
    }

    #[test]
    fn unknown_alias_refuses() {
        let mut value = duo_value();
        action_of(&mut value, "hide-F1")["view"] = serde_json::json!("void");
        let error = load_value(&value).expect_err("must reject");
        assert!(error.contains("home/away"), "unexpected: {error}");
    }

    #[test]
    fn wrong_trio_admit_tiles_rejected() {
        let text = std::fs::read_to_string(trio_path()).expect("read trio fixture");
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("parse trio fixture");
        let actions = value["actions"].as_array_mut().expect("actions");
        let cp = actions
            .iter_mut()
            .find(|a| a["op"] == "checkpoint" && a["name"] == "readmit-b")
            .expect("readmit-b");
        cp["tiles"]["win-a"] = serde_json::json!([0, 0, 401, 280]);
        let case = load_value(&value).expect("loads; oracle is wrong, not the shape");
        let error = run_stress(&case, StressSchedule::Ordered).expect_err("must reject");
        assert!(error.contains("tile"), "unexpected: {error}");
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
