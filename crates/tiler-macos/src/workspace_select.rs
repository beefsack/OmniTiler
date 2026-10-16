#![forbid(unsafe_code)]

//! Select-only native workspace controller. Explicit batches retain two-view
//! history for their lifetime; no transfer, input interception, preference
//! writes, automatic deletion or background lifecycle is implied.

use crate::runtime_types::InputSemanticAction;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSnapshot {
    pub display: String,
    pub spaces: Vec<u64>,
    pub current: u64,
}

impl SelectionSnapshot {
    fn validate(&self) -> Result<(), String> {
        if self.display.is_empty()
            || self.spaces.is_empty()
            || self.spaces.contains(&0)
            || !self.spaces.contains(&self.current)
            || self
                .spaces
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.spaces.len()
        {
            return Err("workspace row/current identity unprovable".to_owned());
        }
        Ok(())
    }
}

pub trait SelectionBackend {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String>;
    fn empty(&mut self, sid: u64) -> Result<bool, String>;
    /// One create dispatch only; caller independently adopts an exact append.
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String>;
    /// One select dispatch only; caller independently verifies arrival.
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), String>;
}

#[derive(Default)]
pub struct SelectionState {
    previous: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SelectionEffect {
    pub from: u64,
    pub target: u64,
    pub created: Option<u64>,
    pub changed: bool,
}

impl SelectionState {
    pub fn execute(
        &mut self,
        backend: &mut impl SelectionBackend,
        action: InputSemanticAction,
    ) -> Result<SelectionEffect, String> {
        let before = backend.snapshot()?;
        before.validate()?;
        let mut created = None;
        let target = match action {
            InputSemanticAction::WorkspaceSelect { index: 0 } => {
                let last = *before.spaces.last().expect("validated nonempty row");
                let empty = backend.empty(last)?;
                if before.spaces.len() >= tiler_core::workspace::MIN_WORKSPACES
                    && tiler_core::workspace::trailing_reusable(empty, !empty)
                {
                    last
                } else {
                    if before.spaces.len() >= tiler_core::workspace::MAX_ORDERED {
                        return Err("shared workspace catalogue bound reached".to_owned());
                    }
                    backend.create(&before)?;
                    let after = backend.snapshot()?;
                    after.validate()?;
                    if after.display != before.display
                        || after.current != before.current
                        || after.spaces.len() != before.spaces.len() + 1
                        || !after.spaces.starts_with(&before.spaces)
                    {
                        return Err(
                            "create effect is not an exact same-display append; adoption refused"
                                .to_owned(),
                        );
                    }
                    let sid = *after.spaces.last().expect("appended row");
                    created = Some(sid);
                    sid
                }
            }
            InputSemanticAction::WorkspaceSelect { index } => {
                let pos = tiler_core::workspace::select_existing(before.spaces.len(), index)
                    .ok_or("numbered workspace does not exist")?;
                before.spaces[pos]
            }
            InputSemanticAction::WorkspaceRelative { prev } => {
                let pos = before
                    .spaces
                    .iter()
                    .position(|s| *s == before.current)
                    .expect("validated current");
                let next = if prev {
                    (pos + before.spaces.len() - 1) % before.spaces.len()
                } else {
                    (pos + 1) % before.spaces.len()
                };
                before.spaces[next]
            }
            InputSemanticAction::WorkspacePrevious => {
                let sid = *self
                    .previous
                    .get(&before.display)
                    .ok_or("no previous workspace in this controller lifetime")?;
                if !before.spaces.contains(&sid) {
                    return Err("previous workspace no longer exists".to_owned());
                }
                sid
            }
            _ => {
                return Err(
                    "select-only controller refuses window-transfer/basic actions".to_owned(),
                );
            }
        };
        let now = backend.snapshot()?;
        now.validate()?;
        let mut expected = before.spaces.clone();
        if let Some(sid) = created {
            expected.push(sid);
        }
        if now.display != before.display || now.spaces != expected || now.current != before.current
        {
            return Err(format!(
                "workspace topology changed before select; created SID {created:?}; no select dispatched"
            ));
        }
        if target != now.current {
            backend
                .select(&now, target)
                .map_err(|e| format!("{e}; created SID {created:?}"))?;
            let after = backend.snapshot()?;
            if after.display != now.display || after.spaces != now.spaces || after.current != target
            {
                return Err(format!(
                    "select effect unproven; created SID {created:?}; history unchanged"
                ));
            }
            self.previous.insert(before.display, before.current);
        }
        Ok(SelectionEffect {
            from: before.current,
            target,
            created,
            changed: target != before.current,
        })
    }
}

/// Select-only binding subset. Never adds send chords; the preview does not
/// arm this subset until empty-Space eligibility and effect routing exist.
#[must_use]
pub fn bindings() -> Vec<crate::runtime_types::TileBinding> {
    crate::workspace_input::workspace_catalogue()
        .into_iter()
        .filter(|row| {
            matches!(
                row.action,
                InputSemanticAction::WorkspaceSelect { .. }
                    | InputSemanticAction::WorkspaceRelative { .. }
                    | InputSemanticAction::WorkspacePrevious
            )
        })
        .filter_map(|row| row.binding())
        .collect()
}

/// After shell classification, zero-area candidate membership is ambiguous,
/// not proof of either a reusable empty Space or an occupied document.
pub(crate) fn candidate_occupied(
    wid: u32,
    pid: u32,
    w: f64,
    h: f64,
    spaces: &[u64],
    sid: u64,
) -> Result<bool, String> {
    if !spaces.contains(&sid) {
        return Ok(false);
    }
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(format!(
            "trailing emptiness unknown: zero-area/unreadable candidate WID{wid}/PID{pid}"
        ));
    }
    Ok(true)
}

pub fn run(display: usize, actions: &[InputSemanticAction]) -> Result<String, (i32, String)> {
    if actions.is_empty()
        || actions.len() > 16
        || actions.iter().any(|a| {
            !matches!(
                a,
                InputSemanticAction::WorkspaceSelect { .. }
                    | InputSemanticAction::WorkspaceRelative { .. }
                    | InputSemanticAction::WorkspacePrevious
            )
        })
    {
        return Err((
            2,
            "spaces-select needs 1..=16 select-only --action IDs (bounded native batch)".to_owned(),
        ));
    }
    crate::mc_lifecycle::live::action_gates().map_err(|e| (1, e.to_string()))?;
    let mut awake =
        crate::mc_lifecycle::live::CaffeinateGuard::spawn(120).map_err(|e| (1, e.to_string()))?;
    let mut backend = NativeSelection {
        display,
        budget: crate::mc_probe::ActionBudget::new(120),
        created: Vec::new(),
    };
    let mut state = SelectionState::default();
    let mut effects = Vec::new();
    let mut failure = None;
    for action in actions {
        let start = std::time::Instant::now();
        match state.execute(&mut backend, *action) {
            Ok(effect) => {
                effects.push(serde_json::json!({"effect":effect,"ms":start.elapsed().as_millis()}))
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let closed = crate::mc_lifecycle::live::mc_close().map_err(|e| e.to_string());
    let layout_after = crate::mc_lifecycle::live::fresh_snapshot()
        .map(|s| s.sanitized())
        .map_err(|e| e.to_string());
    let reaped = awake.stop();
    let report = serde_json::json!({"action":"spaces-select","effects":effects,"failure":failure,"created_sids":backend.created,"layout_after":layout_after,
        "mc_close":closed,"caffeinate_reaped":reaped,
        "note":"requested selection/creation persists; no transfer, deletion, settings writes or hotkeys; history lasts this batch only"}).to_string();
    if failure.is_none() && closed.is_ok() && layout_after.is_ok() && reaped {
        Ok(report)
    } else {
        Err((1, report))
    }
}

struct NativeSelection {
    display: usize,
    budget: crate::mc_probe::ActionBudget,
    created: Vec<u64>,
}

/// Explicit exact-SID cleanup, including batches that appended more than one
/// Space. Existing two-Space `mc-recover` deliberately keeps its strict shape.
/// No implicit selection or broad cleanup: occupied/current/last/unknown refuses.
pub fn remove_empty(space: u64, allow_untested: bool) -> Result<String, (i32, String)> {
    if space == 0 || !allow_untested {
        return Err((
            2,
            "mc-remove-empty needs nonzero SID and --allow-untested".to_owned(),
        ));
    }
    use crate::mc_lifecycle::live;
    live::action_gates().map_err(|e| (1, e.to_string()))?;
    let protected =
        crate::mc_lifecycle::snapshot_ghostty_windows().map_err(|e| (1, e.to_string()))?;
    let mut awake = live::CaffeinateGuard::spawn(60).map_err(|e| (1, e.to_string()))?;
    let mut budget = crate::mc_probe::ActionBudget::new(60);
    let mut dispatched = false;
    let result =
        crate::mc_lifecycle::remove_sid_verified(space, &mut budget, false, &mut dispatched)
            .map(|(scan, close)| serde_json::json!({"scan":scan,"close_shortfall":close}))
            .map_err(|e| e.to_string());
    let closed = live::mc_close().map_err(|e| e.to_string());
    let protected_restored = crate::mc_observe::await_ghostty_restored(&protected).is_ok();
    let reaped = awake.stop();
    let ok = result
        .as_ref()
        .is_ok_and(|v| v["close_shortfall"].is_null())
        && closed.is_ok()
        && protected_restored
        && reaped;
    let body = serde_json::json!({"action":"mc-remove-empty","space":space,"result":result,
        "remove_dispatched":dispatched,"mc_close":closed,"protected_restored":protected_restored,"caffeinate_reaped":reaped,"complete":ok}).to_string();
    if ok { Ok(body) } else { Err((1, body)) }
}

impl SelectionBackend for NativeSelection {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        let all = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let row = all
            .displays
            .get(self.display)
            .ok_or("display row missing")?;
        let mut spaces = Vec::new();
        for sid in &row.spaces {
            match crate::ffi::sls_space_type(*sid).map_err(|e| e.to_string())? {
                crate::ffi::SlsSpaceType::User => spaces.push(*sid),
                _ => continue,
            }
        }
        Ok(SelectionSnapshot {
            display: row
                .identifier
                .clone()
                .ok_or("display identity unreadable")?,
            spaces,
            current: row.current.ok_or("current Space unreadable")?,
        })
    }
    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        let user = crate::mc_observe::fresh_user_spaces().map_err(|e| e.to_string())?;
        let all = crate::mc_observe::enumerate_window_members().map_err(|e| e.to_string())?;
        let mut empty = true;
        for w in all {
            let member = crate::mc_probe::WindowMember {
                wid: w.wid,
                pid: w.pid,
                bundle: w.bundle,
                layer: w.layer,
                onscreen: w.onscreen,
                w: w.w,
                h: w.h,
                exe: w.exe,
                exe_alias: w.exe_alias,
            };
            if crate::mc_probe::classify_window_full(
                &member,
                w.spaces.as_ref().ok().map(Vec::as_slice),
                &user,
                sid,
            )
            .0 == crate::mc_probe::WindowClass::Shell
            {
                continue;
            }
            let spaces = w
                .spaces
                .map_err(|_| "workspace emptiness unknown".to_owned())?;
            if candidate_occupied(member.wid, member.pid, member.w, member.h, &spaces, sid)? {
                empty = false;
            }
        }
        Ok(empty)
    }
    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), String> {
        if self.snapshot()? != *before {
            return Err("pre-create topology drift".to_owned());
        }
        let raw = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let outcome =
            crate::mc_lifecycle::live::mc_add_space(self.display, &mut self.budget, false)
                .map_err(|e| e.to_string())?;
        let after = crate::mc_lifecycle::live::await_snapshot(&|s| {
            crate::mc_probe::validate_create_effect(&raw, s, self.display).is_ok()
        })
        .map_err(|e| e.to_string())?;
        let sid = crate::mc_probe::validate_create_effect(&raw, &after, self.display)
            .map_err(|e| e.to_string())?;
        self.created.push(sid);
        if let Some(e) = outcome.close_error {
            return Err(format!("created but MC close unproven: {e}"));
        }
        Ok(())
    }
    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), String> {
        if self.snapshot()? != *before {
            return Err("pre-select topology drift".to_owned());
        }
        let raw = crate::mc_lifecycle::live::fresh_snapshot().map_err(|e| e.to_string())?;
        let (_, close) = crate::mc_lifecycle::goto_sid_verified(
            &raw,
            self.display,
            sid,
            &mut self.budget,
            false,
        )
        .map_err(|e| e.to_string())?;
        if let Some(e) = close {
            return Err(format!("selected but MC close unproven: {e}"));
        }
        Ok(())
    }
}
