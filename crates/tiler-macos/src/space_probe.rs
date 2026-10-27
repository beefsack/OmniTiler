#![forbid(unsafe_code)]

//! Owned native-Space probe, return, and restore orchestration.
//!
//! The leg's return route must be proven by execution, not by reading back
//! an unchanged current: [`probe_return_to_starting`] creates a newly
//! appended OWNED Space on the starting display, moves away to it and back
//! with exact-effect readbacks, then removes only that Space after proving
//! it empty. [`restore_starting_currents`] is the separate teardown restore:
//! a no-op readback when already exact, one goto attempt per drifted display
//! otherwise, never a creation. [`remove_owned_space`] removes exactly one
//! leg-created Space after pinning its display and proving emptiness.
//!
//! The orchestration below is shared by production and tests: production
//! runs it through [`LiveNativeSpaces`] (the existing owned public Mission
//! Control route: add press, SID-accepting goto presses with exact-effect
//! readbacks, only-empty removal; zero window writes, zero synthetic
//! posts, never replayed). Tests execute the same functions against an
//! in-memory model that enforces the same invariants (exact append,
//! locate-by-SID, emptiness), with rogue-table and per-step failure
//! injection. First-live-proof is still required: no offline run proves the
//! native route.
//!
//! Failure rule everywhere: a failed probe or restore still attempts the
//! starting-Space return and the owned-Space cleanup, collects residue, and
//! refuses. Cleanup never forces a removal it cannot prove empty.

use serde::{Deserialize, Serialize};

use crate::live_baseline::{NativeBaseline, NativeDisplayBaseline};

/// Plain Space topology for the seam: ordered per-display SIDs plus the
/// current Space. Currents are mandatory here (no goto-back target without
/// one), matching the baseline capture rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceTopology {
    pub displays: Vec<SpaceTopologyDisplay>,
}

/// One display row of the seam topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceTopologyDisplay {
    pub spaces: Vec<u64>,
    pub current: u64,
}

impl SpaceTopology {
    /// Build from a captured baseline (every baseline display row carries a
    /// proven current).
    #[must_use]
    pub fn from_baseline(native: &NativeBaseline) -> Self {
        Self {
            displays: native
                .displays
                .iter()
                .map(|row| SpaceTopologyDisplay {
                    spaces: row.spaces.clone(),
                    current: row.current,
                })
                .collect(),
        }
    }

    /// Locate one SID as (display, index). `None` is unknown/ambiguous.
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
}

/// Narrow native seam. Production ([`LiveNativeSpaces`]) calls the existing
/// owned Mission Control route; each method verifies its own effect before
/// returning (exact append, validated goto, proven absence), so the
/// orchestration never trusts a dispatch receipt.
pub trait NativeSpaces {
    /// Fresh display/Space/current rows.
    fn space_snapshot(&mut self) -> Result<SpaceTopology, String>;
    /// Create exactly one Space on `display`; returns the appended SID after
    /// verifying the append (one new SID, currents unchanged).
    fn space_create(&mut self, display: usize) -> Result<u64, String>;
    /// Go to `sid` on `display` with the exact-effect readback.
    fn space_goto(&mut self, display: usize, sid: u64) -> Result<(), String>;
    /// Remove exactly `sid` after proving it empty, then verify absence.
    /// Never replayed, never forced.
    fn space_remove(&mut self, sid: u64) -> Result<(), String>;
    /// Prove `sid` currently holds no tiler-relevant occupants.
    fn space_require_empty(&mut self, sid: u64) -> Result<(), String>;
    /// Mission Control presence readback (`None` when unreadable; never a
    /// claim). Closing presence after a probe or restore.
    fn mc_present(&mut self) -> Option<bool>;
}

/// Executed probe evidence: the display, the owned temp SID, the starting
/// SID, and the step names that actually ran. No titles, no content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceProbeProof {
    pub display: usize,
    pub temp: u64,
    pub starting: u64,
    pub steps: Vec<String>,
}

/// Probe failure: what was attempted plus the residue (owned temp remnants,
/// unrestored currents). The caller retains the receipt and reports this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeFailure {
    pub stage: &'static str,
    pub error: String,
    pub residue: Vec<String>,
}

impl std::fmt::Display for ProbeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "space probe refused at {}: {}; residue: {}",
            self.stage,
            self.error,
            self.residue.join("; ")
        )
    }
}

/// Attempt the owned-temp cleanup after a probe failure: return to the
/// starting current, then remove the temp only when still provably empty.
/// Every leg is attempted; failures collect as residue. Never forces.
fn cleanup_probe_space<S: NativeSpaces>(
    spaces: &mut S,
    display: usize,
    starting: u64,
    temp: Option<u64>,
    residue: &mut Vec<String>,
) {
    if let Err(e) = spaces.space_goto(display, starting) {
        residue.push(format!(
            "probe cleanup: return to starting Space {starting} refused ({e})"
        ));
    }
    if let Some(temp) = temp {
        match spaces.space_require_empty(temp) {
            Ok(()) => {
                if let Err(e) = spaces.space_remove(temp) {
                    residue.push(format!(
                        "probe cleanup: owned temp Space {temp} removal refused ({e}); residue: temp Space remains"
                    ));
                }
            }
            Err(e) => residue.push(format!(
                "probe cleanup: owned temp Space {temp} not provably empty ({e}); residue: temp Space remains, never forced"
            )),
        }
    }
}

/// Execute the pre-mutation return proof: snapshot, create one owned temp on
/// display 0, prove it empty, go away to it, return to the starting current,
/// remove only it after re-proving emptiness, and verify the final currents
/// equal the starting currents. A failure at any step still attempts the
/// starting return and the owned-temp cleanup before refusing.
pub fn probe_return_to_starting<S: NativeSpaces>(
    spaces: &mut S,
    starting: &NativeBaseline,
) -> Result<SpaceProbeProof, ProbeFailure> {
    let fail = |stage: &'static str, error: String, residue: Vec<String>| ProbeFailure {
        stage,
        error,
        residue,
    };
    let starting_topo = SpaceTopology::from_baseline(starting);
    let Some(first) = starting_topo.displays.first() else {
        return Err(fail(
            "probe-entry",
            "starting topology has no displays; refusing".to_owned(),
            Vec::new(),
        ));
    };
    let display = 0;
    let starting_sid = first.current;
    let mut steps = Vec::new();
    // Entry: the current must already be the starting current; a drifted
    // entry restores once before the probe (one attempt, never a loop).
    match spaces.space_snapshot() {
        Ok(before) => {
            steps.push("snapshot-before".to_owned());
            if before.displays.get(display).map(|row| row.current) != Some(starting_sid) {
                match spaces.space_goto(display, starting_sid) {
                    Ok(()) => steps.push("entry-restore".to_owned()),
                    Err(e) => {
                        return Err(fail(
                            "probe-entry",
                            format!("starting current unrestored at probe entry ({e})"),
                            vec![format!(
                                "residue: display {display} not on starting Space {starting_sid}"
                            )],
                        ));
                    }
                }
            }
        }
        Err(e) => {
            return Err(fail(
                "probe-entry",
                format!("pre-probe snapshot unreadable ({e})"),
                vec!["residue: starting topology unknown".to_owned()],
            ));
        }
    }
    // Create the owned temp, then verify the append independently of the
    // seam's own check: exactly one new SID on this display, currents
    // untouched everywhere. A rogue append refuses here, never probed.
    let temp = match spaces.space_create(display) {
        Ok(temp) => {
            steps.push("create-temp".to_owned());
            temp
        }
        Err(e) => {
            return Err(fail(
                "probe-create",
                format!("owned temp Space creation refused ({e})"),
                vec!["residue: no temp created, starting Space untouched".to_owned()],
            ));
        }
    };
    match spaces.space_snapshot() {
        Ok(created) => {
            let entry_spaces = &starting_topo.displays[display].spaces;
            let created_row = created.displays.get(display);
            let appended: Vec<u64> = created_row
                .map(|row| {
                    row.spaces
                        .iter()
                        .filter(|sid| !entry_spaces.contains(sid))
                        .copied()
                        .collect()
                })
                .unwrap_or_default();
            let currents_steady = created
                .displays
                .iter()
                .zip(starting_topo.displays.iter())
                .all(|(got, want)| got.current == want.current);
            let exact_append = appended.len() == 1
                && appended[0] == temp
                && created_row.map(|row| row.spaces.len()) == Some(entry_spaces.len() + 1)
                && currents_steady;
            if !exact_append {
                let mut residue = vec![format!(
                    "residue: display {display} Space set is not exactly one append of {temp}; refusing without probing"
                )];
                cleanup_probe_space(spaces, display, starting_sid, Some(temp), &mut residue);
                return Err(fail(
                    "probe-create",
                    "post-create Space set is not exactly one append".to_owned(),
                    residue,
                ));
            }
            steps.push("verify-append".to_owned());
        }
        Err(e) => {
            let mut residue = Vec::new();
            cleanup_probe_space(spaces, display, starting_sid, Some(temp), &mut residue);
            return Err(fail(
                "probe-create",
                format!("post-create snapshot unreadable ({e})"),
                residue,
            ));
        }
    }
    // The temp must be provably empty before any goto lands on it.
    if let Err(e) = spaces.space_require_empty(temp) {
        let mut residue = Vec::new();
        cleanup_probe_space(spaces, display, starting_sid, Some(temp), &mut residue);
        return Err(fail(
            "probe-empty",
            format!("owned temp Space {temp} not provably empty ({e})"),
            residue,
        ));
    }
    steps.push("temp-empty".to_owned());
    // Away: the proof must exercise a DIFFERENT Space.
    if let Err(e) = spaces.space_goto(display, temp) {
        let mut residue = Vec::new();
        cleanup_probe_space(spaces, display, starting_sid, Some(temp), &mut residue);
        return Err(fail(
            "probe-away",
            format!("goto owned temp Space {temp} refused ({e})"),
            residue,
        ));
    }
    steps.push("goto-away".to_owned());
    // Back: return to the starting current with the exact-effect readback.
    if let Err(e) = spaces.space_goto(display, starting_sid) {
        let mut residue = Vec::new();
        cleanup_probe_space(spaces, display, starting_sid, Some(temp), &mut residue);
        return Err(fail(
            "probe-back",
            format!("return to starting Space {starting_sid} refused ({e})"),
            residue,
        ));
    }
    steps.push("goto-back".to_owned());
    // Remove only the owned temp after re-proving emptiness.
    match spaces.space_require_empty(temp) {
        Ok(()) => match spaces.space_remove(temp) {
            Ok(()) => steps.push("remove-temp".to_owned()),
            Err(e) => {
                return Err(fail(
                    "probe-remove",
                    format!("owned temp Space {temp} removal refused ({e})"),
                    vec![format!("residue: owned temp Space {temp} remains")],
                ));
            }
        },
        Err(e) => {
            return Err(fail(
                "probe-remove",
                format!("owned temp Space {temp} not empty before removal ({e})"),
                vec![format!(
                    "residue: owned temp Space {temp} remains, never forced"
                )],
            ));
        }
    }
    // Final: every display current must equal its starting current.
    match spaces.space_snapshot() {
        Ok(final_topo) => {
            for (index, row) in starting_topo.displays.iter().enumerate() {
                if final_topo.displays.get(index).map(|r| r.current) != Some(row.current) {
                    let mut residue = vec![format!(
                        "residue: display {index} current not restored to starting {}",
                        row.current
                    )];
                    cleanup_probe_space(spaces, display, starting_sid, None, &mut residue);
                    return Err(fail(
                        "probe-verify",
                        format!("final currents != starting currents on display {index}"),
                        residue,
                    ));
                }
            }
            steps.push("verify-currents".to_owned());
        }
        Err(e) => {
            let mut residue = Vec::new();
            cleanup_probe_space(spaces, display, starting_sid, None, &mut residue);
            return Err(fail(
                "probe-verify",
                format!("final snapshot unreadable ({e})"),
                residue,
            ));
        }
    }
    Ok(SpaceProbeProof {
        display,
        temp,
        starting: starting_sid,
        steps,
    })
}

/// Per-display restore outcome. A failed snapshot marks every display
/// failed; drifted displays get exactly one goto attempt each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    pub already_exact: Vec<(usize, u64)>,
    pub restored: Vec<(usize, u64)>,
    pub failed: Vec<String>,
}

impl RestoreReport {
    /// True only when every display is exact and nothing failed.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Restore the starting currents: no-op readback when already exact, one
/// goto attempt per drifted display, never a creation. Separate from the
/// probe: this proves nothing about the route, it only returns selection.
pub fn restore_starting_currents<S: NativeSpaces>(
    spaces: &mut S,
    starting: &NativeBaseline,
) -> RestoreReport {
    let mut report = RestoreReport {
        already_exact: Vec::new(),
        restored: Vec::new(),
        failed: Vec::new(),
    };
    let current = match spaces.space_snapshot() {
        Ok(topo) => topo,
        Err(e) => {
            for (index, _) in starting.displays.iter().enumerate() {
                report.failed.push(format!(
                    "display {index}: restore snapshot unreadable ({e})"
                ));
            }
            return report;
        }
    };
    for row in &starting.displays {
        match current.displays.get(row.index).map(|r| r.current) {
            Some(current_sid) if current_sid == row.current => {
                report.already_exact.push((row.index, row.current));
            }
            _ => match spaces.space_goto(row.index, row.current) {
                Ok(()) => report.restored.push((row.index, row.current)),
                Err(e) => report.failed.push(format!(
                    "display {}: return to starting Space {} refused ({e})",
                    row.index, row.current
                )),
            },
        }
    }
    report
}

/// Remove exactly one leg-created Space: pin it to its creation display,
/// re-prove emptiness, remove, and let the seam verify absence. Anything
/// else (unlocatable, moved display, occupied) refuses with residue, never
/// a forced removal.
pub fn remove_owned_space<S: NativeSpaces>(
    spaces: &mut S,
    display: usize,
    sid: u64,
) -> Result<(), String> {
    let topo = spaces
        .space_snapshot()
        .map_err(|e| format!("owned-space removal: snapshot unreadable ({e}); refusing"))?;
    match topo.locate(sid) {
        Some((found_display, _)) if found_display == display => {}
        Some((found_display, _)) => {
            return Err(format!(
                "owned-space removal: Space {sid} now on display {found_display} (created on {display}); ownership uncertain, refusing"
            ));
        }
        None => {
            return Err(format!(
                "owned-space removal: Space {sid} not uniquely located; ownership uncertain, refusing"
            ));
        }
    }
    spaces.space_require_empty(sid).map_err(|e| {
        format!("owned-space removal: Space {sid} not provably empty ({e}); never forced")
    })?;
    spaces.space_remove(sid).map_err(|e| {
        format!(
            "owned-space removal: Space {sid} removal refused ({e}); residue: owned Space remains"
        )
    })
}

/// Production seam over the existing owned Mission Control route. Every
/// method verifies its own effect with a fresh readback before returning.
pub struct LiveNativeSpaces {
    budget_secs: u64,
}

impl LiveNativeSpaces {
    /// Budget for one probe/restore sequence (genuine deadline: bounded
    /// Mission Control automation, never an open-ended automation run).
    #[must_use]
    pub fn new(budget_secs: u64) -> Self {
        Self { budget_secs }
    }

    fn budget(&self) -> crate::mc_probe::ActionBudget {
        crate::mc_probe::ActionBudget::new(self.budget_secs)
    }
}

impl NativeSpaces for LiveNativeSpaces {
    fn space_snapshot(&mut self) -> Result<SpaceTopology, String> {
        let rows = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("SLS snapshot unreadable ({e})"))?;
        let mut displays = Vec::with_capacity(rows.displays.len());
        for row in &rows.displays {
            let Some(current) = row.current else {
                return Err("snapshot display without a current Space; refusing".to_owned());
            };
            displays.push(SpaceTopologyDisplay {
                spaces: row.spaces.clone(),
                current,
            });
        }
        if displays.is_empty() {
            return Err("snapshot holds no displays; refusing".to_owned());
        }
        Ok(SpaceTopology { displays })
    }

    fn space_create(&mut self, display: usize) -> Result<u64, String> {
        let mut budget = self.budget();
        let before = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("pre-create snapshot unreadable ({e})"))?;
        let before_row = before.displays.get(display).ok_or_else(|| {
            format!(
                "display {display} unknown ({} displays); refusing",
                before.displays.len()
            )
        })?;
        let before_spaces = before_row.spaces.clone();
        let before_current = before_row.current;
        crate::mc_lifecycle::live::mc_add_space(display, &mut budget, false)
            .map_err(|e| format!("add-Space press refused ({e})"))?;
        // Exact-append verification on a fresh readback: exactly one new
        // SID on this display, currents untouched, nothing else moved.
        let after = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("post-create snapshot unreadable ({e})"))?;
        let after_row = after
            .displays
            .get(display)
            .ok_or_else(|| format!("display {display} gone after create; refusing"))?;
        if after_row.current != before_current {
            return Err("post-create current moved; refusing without an exact append".to_owned());
        }
        let mut appended: Vec<u64> = after_row
            .spaces
            .iter()
            .filter(|sid| !before_spaces.contains(sid))
            .copied()
            .collect();
        appended.sort_unstable();
        appended.dedup();
        if appended.len() != 1 || after_row.spaces.len() != before_spaces.len() + 1 {
            return Err("post-create Space set is not exactly one append; refusing".to_owned());
        }
        // The new SID must be uniquely locatable (never ambiguous).
        let topo = SpaceTopology {
            displays: after
                .displays
                .iter()
                .map(|row| SpaceTopologyDisplay {
                    spaces: row.spaces.clone(),
                    current: row.current.unwrap_or(0),
                })
                .collect(),
        };
        match topo.locate(appended[0]) {
            Some((found_display, _)) if found_display == display => Ok(appended[0]),
            _ => Err("appended Space not uniquely located on its display; refusing".to_owned()),
        }
    }

    fn space_goto(&mut self, display: usize, sid: u64) -> Result<(), String> {
        let mut budget = self.budget();
        let before = crate::mc_lifecycle::live::fresh_snapshot()
            .map_err(|e| format!("pre-goto snapshot unreadable ({e})"))?;
        crate::mc_lifecycle::goto_sid_verified(&before, display, sid, &mut budget, false)
            .map(|_| ())
            .map_err(|e| format!("goto Space {sid} refused ({e})"))
    }

    fn space_remove(&mut self, sid: u64) -> Result<(), String> {
        let mut budget = self.budget();
        let mut dispatched = false;
        crate::mc_lifecycle::remove_sid_verified(sid, &mut budget, false, &mut dispatched)
            .map(|_| ())
            .map_err(|e| format!("remove Space {sid} refused ({e})"))
    }

    fn space_require_empty(&mut self, sid: u64) -> Result<(), String> {
        crate::mc_observe::verify_space_empty(sid)
            .map(|_| ())
            .map_err(|e| format!("Space {sid} not provably empty ({e})"))
    }

    fn mc_present(&mut self) -> Option<bool> {
        crate::mc_lifecycle::live::mc_group_present().ok()
    }
}

/// Native display baseline helper for tests and adapters.
#[must_use]
pub fn native_baseline_for_test(displays: Vec<(Vec<u64>, u64)>, separate: bool) -> NativeBaseline {
    NativeBaseline {
        separate_spaces: separate,
        displays: displays
            .into_iter()
            .enumerate()
            .map(|(index, (spaces, current))| NativeDisplayBaseline {
                index,
                spaces,
                current,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Rogue table behavior for exact-append enforcement tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Rogue {
        /// Append two Spaces instead of one.
        DoubleAppend,
        /// Move the current during creation.
        StealCurrent,
    }

    /// Per-step failure injection.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FailAt {
        Snapshot,
        Create,
        Empty,
        GotoTemp,
        GotoBack,
        Remove,
        FinalSnapshot,
    }

    /// In-memory native table enforcing the same invariants production
    /// verifies: ordered per-display SIDs, located currents, emptiness by
    /// occupant count, unique fresh SIDs. Failures and rogue behavior are
    /// injected per step; every call is logged in order.
    struct ModelSpaces {
        spaces: Vec<Vec<u64>>,
        current: Vec<u64>,
        occupants: HashMap<u64, u32>,
        next_sid: u64,
        fail: Option<FailAt>,
        rogue: Option<Rogue>,
        /// SIDs the model treats as occupied (emptiness refuses).
        occupied: Vec<u64>,
        log: Vec<&'static str>,
    }

    impl ModelSpaces {
        fn new(displays: Vec<(Vec<u64>, u64)>) -> Self {
            // Fresh SIDs start past any test starting range so the temp is
            // always distinguishable from pre-existing Spaces.
            let next_sid = displays
                .iter()
                .flat_map(|(spaces, _)| spaces.iter())
                .copied()
                .max()
                .unwrap_or(0)
                .max(999)
                + 1;
            Self {
                spaces: displays.iter().map(|(s, _)| s.clone()).collect(),
                current: displays.iter().map(|(_, c)| *c).collect(),
                occupants: HashMap::new(),
                next_sid,
                fail: None,
                rogue: None,
                occupied: Vec::new(),
                log: Vec::new(),
            }
        }

        fn locate(&self, sid: u64) -> Option<(usize, usize)> {
            let mut found = None;
            for (di, spaces) in self.spaces.iter().enumerate() {
                for (si, s) in spaces.iter().enumerate() {
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
    }

    impl NativeSpaces for ModelSpaces {
        fn space_snapshot(&mut self) -> Result<SpaceTopology, String> {
            self.log.push("snapshot");
            let snapshots = self.log.iter().filter(|l| **l == "snapshot").count();
            // FailAt::Snapshot fails every snapshot; FailAt::FinalSnapshot
            // fails only snapshots after the entry read (the final verify).
            if self.fail == Some(FailAt::Snapshot)
                || (self.fail == Some(FailAt::FinalSnapshot) && snapshots >= 2)
            {
                return Err("model snapshot refused".to_owned());
            }
            self.snapshot_now()
        }

        fn space_create(&mut self, display: usize) -> Result<u64, String> {
            self.log.push("create");
            if self.fail == Some(FailAt::Create) {
                return Err("model create refused".to_owned());
            }
            let row = self
                .spaces
                .get_mut(display)
                .ok_or_else(|| format!("model display {display} unknown"))?;
            let temp = self.next_sid;
            self.next_sid += 1;
            row.push(temp);
            if self.rogue == Some(Rogue::DoubleAppend) {
                let extra = self.next_sid;
                self.next_sid += 1;
                row.push(extra);
            }
            if self.rogue == Some(Rogue::StealCurrent) {
                self.current[display] = temp;
            }
            Ok(temp)
        }

        fn space_goto(&mut self, display: usize, sid: u64) -> Result<(), String> {
            let temp_target = self.last_temp();
            if Some(sid) == temp_target {
                self.log.push("goto-temp");
                if self.fail == Some(FailAt::GotoTemp) {
                    return Err("model goto-temp refused".to_owned());
                }
            } else {
                self.log.push("goto-back");
                if self.fail == Some(FailAt::GotoBack) {
                    return Err("model goto-back refused".to_owned());
                }
            }
            match self.locate(sid) {
                Some((found_display, _)) if found_display == display => {
                    self.current[display] = sid;
                    Ok(())
                }
                _ => Err(format!("model Space {sid} not on display {display}")),
            }
        }

        fn space_remove(&mut self, sid: u64) -> Result<(), String> {
            self.log.push("remove");
            if self.fail == Some(FailAt::Remove) {
                return Err("model remove refused".to_owned());
            }
            match self.locate(sid) {
                Some((di, si)) => {
                    self.spaces[di].remove(si);
                    // Absence verification.
                    if self.locate(sid).is_some() {
                        return Err("model Space still present after remove".to_owned());
                    }
                    Ok(())
                }
                None => Err(format!("model Space {sid} unknown")),
            }
        }

        fn space_require_empty(&mut self, sid: u64) -> Result<(), String> {
            self.log.push("empty");
            if self.fail == Some(FailAt::Empty) {
                return Err("model emptiness refused".to_owned());
            }
            if self.occupied.contains(&sid) || self.occupants.get(&sid).copied().unwrap_or(0) > 0 {
                return Err(format!("model Space {sid} occupied"));
            }
            self.locate(sid)
                .map(|_| ())
                .ok_or_else(|| format!("model Space {sid} unknown"))
        }

        fn mc_present(&mut self) -> Option<bool> {
            Some(false)
        }
    }

    impl ModelSpaces {
        fn snapshot_now(&self) -> Result<SpaceTopology, String> {
            Ok(SpaceTopology {
                displays: self
                    .spaces
                    .iter()
                    .zip(self.current.iter())
                    .map(|(spaces, current)| SpaceTopologyDisplay {
                        spaces: spaces.clone(),
                        current: *current,
                    })
                    .collect(),
            })
        }

        fn space_snapshot_no_fail(&mut self) -> Result<SpaceTopology, String> {
            self.snapshot_now()
        }

        fn last_temp(&self) -> Option<u64> {
            // The temp is the max SID on display 0 past the test's known
            // range; tests use starting SIDs below 1000 and temps above.
            self.spaces
                .first()?
                .iter()
                .copied()
                .filter(|sid| *sid >= 1000)
                .max()
        }
    }

    fn starting() -> NativeBaseline {
        native_baseline_for_test(vec![(vec![11, 22], 11), (vec![33], 33)], true)
    }

    fn starting_single() -> NativeBaseline {
        native_baseline_for_test(vec![(vec![11, 22], 11)], true)
    }

    #[test]
    fn probe_executes_away_and_back_on_owned_temp() {
        let mut model = ModelSpaces::new(vec![(vec![11, 22], 11), (vec![33], 33)]);
        let proof = probe_return_to_starting(&mut model, &starting()).expect("probe proves");
        assert_eq!(proof.display, 0);
        assert_eq!(proof.starting, 11);
        assert!(proof.temp >= 1000, "temp is newly appended");
        // Every step actually ran, in order, on a different Space.
        assert_eq!(
            proof.steps,
            vec![
                "snapshot-before",
                "create-temp",
                "verify-append",
                "temp-empty",
                "goto-away",
                "goto-back",
                "remove-temp",
                "verify-currents",
            ]
        );
        // The temp is gone and currents are exact.
        let after = model.space_snapshot_no_fail().expect("snapshot");
        assert_eq!(after.displays[0].spaces, vec![11, 22]);
        assert_eq!(after.displays[0].current, 11);
        assert_eq!(after.displays[1].current, 33);
    }

    #[test]
    fn probe_refuses_inexact_append_before_probing() {
        // A rogue double append is refused at the orchestration's own
        // exact-append gate, before any goto lands on the temp. Cleanup
        // still attempts the starting return; the ambiguous extra Space is
        // named residue, never force-removed.
        let mut model = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        model.rogue = Some(Rogue::DoubleAppend);
        let err = probe_return_to_starting(&mut model, &starting()).unwrap_err();
        assert_eq!(err.stage, "probe-create", "unexpected: {err}");
        assert!(
            !model.log.contains(&"goto-temp"),
            "no goto may land before the append verifies: {:?}",
            model.log
        );
        assert!(
            err.residue
                .iter()
                .any(|r| r.contains("not exactly one append")),
            "unexpected residue: {:?}",
            err.residue
        );
        // A current stolen mid-create refuses the same gate.
        let mut steal = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        steal.rogue = Some(Rogue::StealCurrent);
        let err = probe_return_to_starting(&mut steal, &starting()).unwrap_err();
        assert_eq!(err.stage, "probe-create", "unexpected: {err}");
    }

    #[test]
    fn probe_fails_closed_at_every_step_with_cleanup() {
        // Fail the temp-emptiness leg: cleanup returns to starting and the
        // temp is removed (it is empty), then the probe refuses.
        let mut fail_empty = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        fail_empty.fail = Some(FailAt::Empty);
        // First emptiness check fails; the cleanup's re-check also fails,
        // so the temp remains as named residue (never forced).
        let err = probe_return_to_starting(&mut fail_empty, &starting_single()).unwrap_err();
        assert_eq!(err.stage, "probe-empty");
        assert!(
            err.residue.iter().any(|r| r.contains("remains")),
            "unexpected residue: {:?}",
            err.residue
        );
        // Fail the away goto: cleanup must still attempt return + removal.
        let mut fail_away = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        fail_away.fail = Some(FailAt::GotoTemp);
        let err = probe_return_to_starting(&mut fail_away, &starting_single()).unwrap_err();
        assert_eq!(err.stage, "probe-away");
        assert_eq!(fail_away.current[0], 11, "cleanup returns to starting");
        assert_eq!(
            fail_away.spaces[0],
            vec![11, 22],
            "cleanup removes the temp"
        );
        // Fail the back goto: the temp is removed during cleanup.
        let mut fail_back = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        fail_back.fail = Some(FailAt::GotoBack);
        let err = probe_return_to_starting(&mut fail_back, &starting_single()).unwrap_err();
        assert_eq!(err.stage, "probe-back");
        assert_eq!(fail_back.spaces[0], vec![11, 22]);
        // Fail removal: the temp remains as named residue, never retried.
        let mut fail_remove = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        fail_remove.fail = Some(FailAt::Remove);
        let err = probe_return_to_starting(&mut fail_remove, &starting_single()).unwrap_err();
        assert!(err.stage == "probe-remove" || err.stage == "probe-verify");
        assert!(
            err.residue.iter().any(|r| r.contains("remains")),
            "unexpected residue: {:?}",
            err.residue
        );
        assert_eq!(
            fail_remove.log.iter().filter(|l| **l == "remove").count(),
            1,
            "removal never replayed"
        );
        // Fail creation: no temp exists, starting untouched.
        let mut fail_create = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        fail_create.fail = Some(FailAt::Create);
        let err = probe_return_to_starting(&mut fail_create, &starting_single()).unwrap_err();
        assert_eq!(err.stage, "probe-create");
        assert_eq!(fail_create.spaces[0], vec![11, 22]);
    }

    #[test]
    fn occupied_temp_is_never_forced() {
        let mut model = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        // Occupy every future temp SID the model can mint.
        model.occupied = vec![1000, 1001, 1002, 1003];
        let err = probe_return_to_starting(&mut model, &starting_single()).unwrap_err();
        assert_eq!(err.stage, "probe-empty");
        assert!(
            err.residue.iter().any(|r| r.contains("never forced")),
            "unexpected residue: {:?}",
            err.residue
        );
    }

    #[test]
    fn restore_is_noop_when_exact_and_moves_drift() {
        let mut model = ModelSpaces::new(vec![(vec![11, 22], 11), (vec![33], 33)]);
        let report = restore_starting_currents(&mut model, &starting());
        assert!(report.failed.is_empty());
        assert!(report.restored.is_empty());
        assert_eq!(report.already_exact, vec![(0, 11), (1, 33)]);
        // Drift display 0 away, then restore moves exactly it back.
        model.current[0] = 22;
        let report = restore_starting_currents(&mut model, &starting());
        assert!(report.failed.is_empty());
        assert_eq!(report.restored, vec![(0, 11)]);
        assert_eq!(report.already_exact, vec![(1, 33)]);
        assert_eq!(model.current[0], 11);
    }

    #[test]
    fn remove_owned_space_pins_display_and_emptiness() {
        let mut model = ModelSpaces::new(vec![(vec![11, 22], 11)]);
        let temp = model.space_create(0).expect("create");
        // Occupied temp refuses without removal.
        model.occupied.push(temp);
        let err = remove_owned_space(&mut model, 0, temp).unwrap_err();
        assert!(
            err.contains("never forced") || err.contains("not provably empty"),
            "unexpected: {err}"
        );
        assert!(model.locate(temp).is_some(), "occupied temp never removed");
        // Wrong display pin refuses (ownership uncertain).
        model.occupied.clear();
        let err = remove_owned_space(&mut model, 1, temp).unwrap_err();
        assert!(err.contains("ownership uncertain"), "unexpected: {err}");
        // Exact pin plus emptiness removes and verifies absence.
        remove_owned_space(&mut model, 0, temp).expect("removes");
        assert!(model.locate(temp).is_none());
        // Unknown SID refuses.
        assert!(remove_owned_space(&mut model, 0, 999_999).is_err());
    }
}
