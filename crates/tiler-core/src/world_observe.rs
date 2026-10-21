//! Staged observation and logical homing for the world transition.
//!
//! Split from [crate::world_engine]: same impl WorldEngine ownership,
//! only relocated for source size. Stages screens, view echoes, and raw
//! facts into staged domains, assigns every fact a logical homing, and
//! retains the core-owned live topology directional commands resolve
//! against. No behavior change.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Direction, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::world_engine::{StagedDomain, StagedReport, WorldEngine};
use crate::world_operations::parse_direction_token;
use crate::world_report::{HoldReason, TransferKind, WorldReport};

impl WorldEngine {
    /// Retain (or fence) the core-owned live output topology from one
    /// accepted report, before any command planning on that same report.
    /// A report with every screen complete and carrying valid FULL
    /// rectangles replaces the map; anything else (no screens, incomplete
    /// screens, missing or invalid rectangles, duplicate output ids)
    /// clears it, so topology-dependent operations fence until the next
    /// complete report. Screen completeness is independent of unrelated
    /// window-read failures: `inventory_complete` (window inventory) never
    /// gates topology retention. Never infers geometry from work areas or
    /// window frames.
    pub(crate) fn retain_live_topology(&mut self, report: &WorldReport) {
        let mut fresh: BTreeMap<String, Rect> = BTreeMap::new();
        let mut complete = !report.screens.is_empty();
        for screen in &report.screens {
            if !screen.complete {
                complete = false;
                break;
            }
            let Some(rect) = screen.full_rect else {
                complete = false;
                break;
            };
            if screen.output.0.is_empty()
                || !crate::bounds::valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
                || fresh.insert(screen.output.0.clone(), rect).is_some()
            {
                complete = false;
                break;
            }
        }
        if complete {
            self.live_topology = fresh;
        } else {
            self.live_topology.clear();
        }
    }

    /// Live FULL rectangles as shared selector entries, in stable output
    /// order. Empty when no complete topology is retained.
    pub(crate) fn live_topology_entries(&self) -> Vec<crate::output_selection::OutputEntry> {
        self.live_topology
            .iter()
            .map(|(id, rect)| crate::output_selection::OutputEntry {
                id: OutputId(id.clone()),
                rect: *rect,
            })
            .collect()
    }

    /// Staged logical homing for one window, if the staged observation
    /// assigns it to a known logical workspace.
    pub(crate) fn staged_homing(&self, window: &WindowId) -> Option<DomainKey> {
        self.staged.homing.get(window).cloned()
    }

    /// Output owning one logical workspace id, if any ring names it.
    /// Floating-only fallback: a floating window's physical frame stays
    /// put while its logical home crosses outputs, so the physical
    /// `fact.output` no longer names the logical screen. Tiled rows
    /// never use this: physical screen affinity is their arrival proof.
    pub(crate) fn owning_output(&self, workspace: &str) -> Option<OutputId> {
        for (output, ring) in &self.mapping {
            if ring.iter().any(|id| id == workspace) {
                return Some(OutputId(output.clone()));
            }
        }
        None
    }

    /// Screen completeness from the staged observation: `None` when the
    /// screen is not reported at all.
    pub(crate) fn screen_complete(&self, output: &OutputId) -> Option<bool> {        self.staged
            .screens
            .iter()
            .find(|screen| screen.output == *output)
            .map(|screen| screen.complete)
    }

    /// Effective view for one screen: the adapter echo when it names a
    /// known ring member (adapter truth wins), else the retained core view
    /// when it names one, else the ring head. `None` when the screen owns
    /// no ring.
    pub(crate) fn effective_view(&self, output: &OutputId) -> Option<String> {
        let ring = self.mapping.get(&output.0)?;
        if ring.is_empty() {
            return None;
        }
        if let Some(echo) = self.staged.echoes.get(&output.0)
            && ring.iter().any(|id| id == echo)
        {
            return Some(echo.clone());
        }
        if let Some(view) = self.views.get(&output.0)
            && ring.iter().any(|id| id == view)
        {
            return Some(view.clone());
        }
        ring.first().cloned()
    }

    /// Reported echo for one screen, when the adapter names a known ring
    /// member. Unknown echoes never authorize homing or views.
    pub(crate) fn known_echo(&self, output: &OutputId) -> Option<String> {
        let echo = self.staged.echoes.get(&output.0)?;
        if self
            .mapping
            .get(&output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == echo))
        {
            Some(echo.clone())
        } else {
            None
        }
    }

    /// Rehome a retained key off a removed screen onto its displaced
    /// survivor, keeping the logical id. `None` when the key is live or
    /// no live survivor hosts it.
    pub(crate) fn rehome_displaced(&self, key: DomainKey) -> Option<DomainKey> {
        if self.mapping.contains_key(&key.output.0) {
            return Some(key);
        }
        let dest = self.displaced.get(&key.output.0)?.dest.clone();
        if !self.mapping.contains_key(&dest) {
            return None;
        }
        Some(DomainKey {
            output: OutputId(dest),
            workspace: key.workspace,
        })
    }

    /// Index the raw observation for one accepted run: screens, view
    /// echoes, facts, closes, and the gesture barrier. Homing and domains
    /// follow in [`WorldEngine::build_staged_domains`], after rings/views
    /// converge.
    pub(crate) fn stage_facts(&mut self, report: &WorldReport) {
        let mut staged = StagedReport {
            screens: report.screens.clone(),
            gesture_barrier: report.gesture_barrier,
            ..StagedReport::default()
        };
        for view in &report.views {
            if !view.view.is_empty() {
                staged
                    .echoes
                    .insert(view.output.0.clone(), view.view.clone());
            }
        }
        for fact in &report.windows {
            staged
                .fact_output
                .insert(fact.window.clone(), fact.output.clone());
            staged.facts.insert(fact.window.clone(), fact.clone());
        }
        for id in &report.closed {
            staged.closes.insert(id.clone());
        }
        self.staged = staged;
    }

    /// Occupied logical ids on one screen's ring: homed non-sticky
    /// facts, for trailing lifecycle and send-ring resolution.
    pub(crate) fn ring_occupied(&self, output: &OutputId) -> BTreeSet<String> {
        let mut occupied = BTreeSet::new();
        if let Some(ring) = self.mapping.get(&output.0) {
            for id in ring {
                let key = DomainKey {
                    output: output.clone(),
                    workspace: WorkspaceId(id.clone()),
                };
                for (window, homed) in &self.staged.homing {
                    if homed == &key
                        && self
                            .staged
                            .facts
                            .get(window)
                            .is_some_and(|fact| !fact.sticky)
                    {
                        occupied.insert(id.clone());
                        break;
                    }
                }
            }
        }
        occupied
    }

    /// Homing for an explicitly closed id: retained session homing when
    /// known (remapped off removed screens), else `None` (nothing to
    /// forget).
    pub(crate) fn homing_for_closed(&self, window: &WindowId) -> Option<DomainKey> {
        let key = self.retained_homing(window)?;
        self.rehome_displaced(key)
    }

    /// Strip one logical workspace id from every ring except `keep`.
    pub(crate) fn take_workspace_from_all(&mut self, id: &str, keep: &str) {
        for (output, list) in self.mapping.iter_mut() {
            if output != keep {
                list.retain(|kept| kept != id);
            }
        }
    }

    /// Move the retained tiling mode for one logical workspace from one
    /// screen to another, so manual toggles follow displacement and return
    /// instead of reseeding from the saved default.
    pub(crate) fn carry_tiling(&mut self, workspace: &str, from_output: &str, to_output: &str) {
        if from_output == to_output {
            return;
        }
        let from = DomainKey {
            output: OutputId(from_output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        };
        if let Some(mode) = self.tiling.remove(&from) {
            self.tiling.insert(
                DomainKey {
                    output: OutputId(to_output.to_owned()),
                    workspace: WorkspaceId(workspace.to_owned()),
                },
                mode,
            );
        }
    }

    /// Build the staged domains: assign every fact to a logical workspace
    /// and project one [`StagedDomain`] per known logical id. Runs after
    /// ring/view convergence and view settlement, so homing assigns
    /// against adapter truth.
    pub(crate) fn build_staged_domains(&mut self, report: &WorldReport) {
        let mut homing: BTreeMap<WindowId, DomainKey> = BTreeMap::new();
        for fact in &report.windows {
            if let Some(key) = self.logical_homing(report, &fact.window) {
                homing.insert(fact.window.clone(), key);
            }
        }
        self.staged.homing = homing;
        let mut domains: Vec<StagedDomain> = Vec::new();
        let mut rings: Vec<(String, Vec<String>)> = self
            .mapping
            .iter()
            .map(|(output, ring)| (output.clone(), ring.clone()))
            .collect();
        rings.sort_by(|a, b| a.0.cmp(&b.0));
        for (output, ring) in rings {
            let screen = self
                .staged
                .screens
                .iter()
                .find(|screen| screen.output.0 == output);
            let (work_area, gap, outer_gap, complete) = match screen {
                Some(screen) => (
                    screen.work_area,
                    screen.gap,
                    screen.outer_gap,
                    screen.complete,
                ),
                None => (None, 0, 0, false),
            };
            for id in ring {
                let key = DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id),
                };
                let mut windows = Vec::new();
                let mut minimized = Vec::new();
                let mut unreadable = Vec::new();
                for (window, homed) in &self.staged.homing {
                    if homed != &key {
                        continue;
                    }
                    let Some(fact) = self.staged.facts.get(window) else {
                        continue;
                    };
                    if fact.minimized {
                        minimized.push(window.clone());
                    } else if let Some(rect) = fact.rect {
                        windows.push(EngineWindow {
                            window: window.clone(),
                            output: key.output.clone(),
                            workspace: key.workspace.clone(),
                            rect,
                            floating: fact.floating,
                            fit_excluded: fact.fit_excluded,
                            fullscreen: fact.fullscreen,
                            maximized: fact.maximized,
                            sticky: fact.sticky,
                            fixed_auto: fact.fixed_auto,
                            fixed_suppress: fact.fixed_suppress,
                            hints: fact.hints,
                        });
                    } else {
                        unreadable.push(window.clone());
                    }
                }
                let mut closed = Vec::new();
                for id in &self.staged.closes.clone() {
                    if self.homing_for_closed(id).as_ref() == Some(&key) {
                        closed.push(id.clone());
                    }
                }
                let focused = if report.focused_global.is_empty() {
                    WindowId(String::new())
                } else {
                    let active = WindowId(report.focused_global.clone());
                    let homed_here = self.staged.homing.get(&active).as_ref() == Some(&&key);
                    let active_live = self
                        .staged
                        .facts
                        .get(&active)
                        .is_some_and(|fact| !fact.minimized);
                    if homed_here && active_live {
                        active
                    } else {
                        WindowId(String::new())
                    }
                };
                let readable = complete && work_area.is_some() && unreadable.is_empty();
                let hold = if !complete {
                    Some(HoldReason::IncompleteInventory)
                } else if !unreadable.is_empty() {
                    Some(HoldReason::Unreadable)
                } else {
                    None
                };
                domains.push(StagedDomain {
                    key,
                    bounds: work_area.unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    }),
                    gap,
                    outer_gap,
                    windows,
                    minimized,
                    unreadable,
                    closed,
                    readable,
                    hold,
                    focused,
                });
            }
        }
        self.staged.domains = domains;
    }

    /// Retained logical homing for one window: the single-owner core
    /// membership map, remapped off removed screens onto the displaced
    /// survivor. Layout sessions never authorize homing.
    pub(crate) fn retained_homing(&self, window: &WindowId) -> Option<DomainKey> {
        self.core_homing(window)
    }

    /// Logical homing for one observed window against the reported facts:
    /// minimized facts keep their retained session homing when known (else
    /// the screen's effective view); an in-flight window observed on its
    /// target screen homes to the target key (arrival); a resident whose
    /// retained output matches the observed screen keeps its homing (never
    /// yanked by a view change); anything else homes to the observed
    /// screen's effective view (external moves adopted, newcomers placed).
    /// `None` when the window carries no fact on a known screen.
    pub(crate) fn logical_homing(
        &self,
        report: &WorldReport,
        window: &WindowId,
    ) -> Option<DomainKey> {
        let fact = report.windows.iter().find(|fact| fact.window == *window)?;
        // In-flight migration member: the raw screen decides ahead of any
        // echo or session state, so member arrival proves independently of
        // views. Observed at the target screen homes to the target key
        // (arrival); observed at the source screen homes to the source
        // cell (delayed, never yanked by the committed target session or
        // a view change).
        if let Some((source_key, target_key)) = self.retained.iter().find_map(|r| {
            if matches!(&r.target.kind, TransferKind::Migration)
                && r.members.iter().any(|m| m == window)
            {
                Some((
                    DomainKey {
                        output: r.source_output.clone(),
                        workspace: WorkspaceId(r.source_workspace.clone()),
                    },
                    DomainKey {
                        output: r.target.target_output.clone(),
                        workspace: WorkspaceId(r.target.target_workspace.clone()),
                    },
                ))
            } else {
                None
            }
        }) {
            // Truthful member receipts win over the raw screen: an explicit
            // contrary logical receipt must never count as arrival proof.
            // Unknown membership proves arrival only in the native-free
            // default (physical screen semantics); with host-owned domains
            // the adapter later supplies translated logical receipts.
            if fact.output == target_key.output {
                match fact.observed_workspace.as_deref() {
                    Some(observed) if observed == target_key.workspace.0 => {
                        return Some(target_key);
                    }
                    Some(observed) => {
                        return Some(DomainKey {
                            output: fact.output.clone(),
                            workspace: WorkspaceId(observed.to_owned()),
                        });
                    }
                    None if !report.host_owned_workspace_domains => {
                        return Some(target_key);
                    }
                    None => {
                        return Some(source_key);
                    }
                }
            }
            if fact.output == source_key.output {
                match fact.observed_workspace.as_deref() {
                    Some(observed) if observed == source_key.workspace.0 => {
                        return Some(source_key);
                    }
                    Some(observed) => {
                        return Some(DomainKey {
                            output: fact.output.clone(),
                            workspace: WorkspaceId(observed.to_owned()),
                        });
                    }
                    None => {
                        return Some(source_key);
                    }
                }
            }
        }
        if fact.minimized {
            if let Some(key) = self.core_homing(window) {
                return Some(key);
            }
            return self.effective_view(&fact.output).map(|view| DomainKey {
                output: fact.output.clone(),
                workspace: WorkspaceId(view),
            });
        }
        // In-flight arrival: observed on the target screen. Observed
        // logical membership is settlement-only evidence. Same-output
        // sends stay pending under unchanged observations and settle
        // only on observed membership; cross-output arrivals keep the
        // mechanical screen proof when membership is unknown (None), and
        // require the exact target id when the adapter reports one.
        // With host-owned domains unknown membership never proves arrival
        // (the adapter later supplies translated logical receipts).
        if let Some(found) = self.retained.iter().find_map(|r| match &r.target.kind {
            TransferKind::Window(w) if w == window => Some(r.clone()),
            _ => None,
        }) {
            let target = found.target.clone();
            if fact.output == target.target_output {
                let same_output = found.source_output == target.target_output;
                match fact.observed_workspace.as_deref() {
                    Some(observed) if observed == target.target_workspace => {
                        return Some(DomainKey {
                            output: target.target_output,
                            workspace: WorkspaceId(target.target_workspace),
                        });
                    }
                    Some(observed) => {
                        // Floating logical crossing preserves the physical
                        // frame: home the receipt on its owning screen
                        // instead of the physical one.
                        if fact.floating
                            && let Some(owning) = self.owning_output(observed)
                        {
                            return Some(DomainKey {
                                output: owning,
                                workspace: WorkspaceId(observed.to_owned()),
                            });
                        }
                        return Some(DomainKey {
                            output: fact.output.clone(),
                            workspace: WorkspaceId(observed.to_owned()),
                        });
                    }
                    None if !same_output && !report.host_owned_workspace_domains => {
                        return Some(DomainKey {
                            output: target.target_output,
                            workspace: WorkspaceId(target.target_workspace),
                        });
                    }
                    None if !same_output => {
                        return Some(DomainKey {
                            output: found.source_output.clone(),
                            workspace: WorkspaceId(found.source_workspace.clone()),
                        });
                    }
                    None => {}
                }
            }
        }
        // Resident keeps homing while observed on its retained screen;
        // displaced survivors keep their logical id on the survivor. A
        // truthful ordinary receipt naming a known workspace on the
        // observed screen wins over desire (unknown membership never
        // proves anything; unknown ids never authorize). A floating
        // receipt naming a workspace owned by another screen homes
        // there: the physical frame stays put on a logical crossing.
        if let Some(observed) = fact.observed_workspace.as_deref()
            && self.known_workspace(&fact.output, observed)
        {
            return Some(DomainKey {
                output: fact.output.clone(),
                workspace: WorkspaceId(observed.to_owned()),
            });
        }
        if fact.floating
            && let Some(observed) = fact.observed_workspace.as_deref()
            && let Some(owning) = self.owning_output(observed)
        {
            return Some(DomainKey {
                output: owning,
                workspace: WorkspaceId(observed.to_owned()),
            });
        }
        if let Some(key) = self.core_homing(window) {
            if key.output == fact.output {
                return Some(key);
            }
            if let Some(remapped) = self.rehome_displaced(key)
                && remapped.output == fact.output
            {
                return Some(remapped);
            }
        }
        self.effective_view(&fact.output).map(|view| DomainKey {
            output: fact.output.clone(),
            workspace: WorkspaceId(view),
        })
    }
}

/// Core-topology-derived directional pair for a cross-output move/focus:
/// the source domain plus the retained FULL-topology target output in the
/// requested direction (strict unique-reciprocal, never carried adapter
/// adjacency), resolved to the same workspace preferred, else the target
/// output's current view, else the first readable non-held domain there,
/// with the target's validated outer gap. The returned domains carry the
/// core-derived reciprocal pair adjacency (source names target, target
/// names source back), so the Engine's existing reciprocal R4/session
/// contracts validate a core derivation, not host facts. `None` refuses
/// without mutation: no/incomplete/stale topology, no resolvable target,
/// unreadable source, or no readable target.
type DirectionalPair = (
    (OutputDomain, DomainKey),
    (OutputDomain, DomainKey),
    i32,
    Direction,
);

impl WorldEngine {
    /// Strict unique-reciprocal target output for one source in one
    /// direction over the retained live FULL topology. `None` fences:
    /// cleared topology, gaps, or ambiguous edges.
    pub(crate) fn core_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
    ) -> Option<OutputId> {
        let entries = self.live_topology_entries();
        let map = crate::output_selection::unique_reciprocal_adjacency(&entries).ok()?;
        map.get(&(source.clone(), direction)).cloned()
    }

    /// Reciprocal edge existence between two outputs in one direction over
    /// the retained live FULL topology: the forward edge-touch candidate
    /// set contains the target AND the reverse set contains the source.
    /// Reverse uniqueness is NOT required (valid reverse ambiguity still
    /// pairs). `false` on any unreadable topology, unknown ids, or missing
    /// touch either way: callers fence without mutation.
    pub(crate) fn reciprocal_pair_exists(
        &self,
        source: &OutputId,
        target: &OutputId,
        direction: Direction,
    ) -> bool {
        use crate::output_selection::{OutputEntry, candidates_in_direction};
        let entries = self.live_topology_entries();
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let (Some(source_entry), Some(target_entry)) = (
            entries.iter().find(|e| &e.id == source).cloned(),
            entries.iter().find(|e| &e.id == target).cloned(),
        ) else {
            return false;
        };
        let Ok(forward) = candidates_in_direction(&source_entry, direction, &entries) else {
            return false;
        };
        if !forward.iter().any(|c| &c.id == target) {
            return false;
        }
        let back_entry = OutputEntry {
            id: target_entry.id.clone(),
            rect: target_entry.rect,
        };
        let Ok(reverse) = candidates_in_direction(&back_entry, opposite, &entries) else {
            return false;
        };
        reverse.iter().any(|c| &c.id == source)
    }

    /// Reciprocal direction from one output to a named other output, with
    /// the Engine direction token. Forward plus reverse edge-touch must
    /// both hold; reverse uniqueness is not required. `None` fences.
    pub(crate) fn reciprocal_direction_for(
        &self,
        source: &OutputId,
        target: &OutputId,
    ) -> Option<(Direction, &'static str)> {
        const ORDER: [(Direction, &str); 4] = [
            (Direction::Left, "left"),
            (Direction::Right, "right"),
            (Direction::Up, "up"),
            (Direction::Down, "down"),
        ];
        ORDER
            .into_iter()
            .find(|(direction, _)| self.reciprocal_pair_exists(source, target, *direction))
    }

    /// Windowed directional target over retained FULL topology
    /// (2026-10-09: centre projection, then span overlap, then left/top).
    /// `Err` on any unreadable topology (refuse); `Ok(None)` when no
    /// candidate touches or the selected target lacks reverse edge-touch
    /// (caller-side local for moves, refusal for sends). No wrap.
    pub(crate) fn window_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
        window: &crate::geometry::Rect,
    ) -> Result<Option<OutputId>, crate::output_selection::SelectionError> {
        use crate::output_selection::select_for_window;
        let entries = self.live_topology_entries();
        let Some(source_entry) = entries.iter().find(|e| &e.id == source).cloned() else {
            return Err(crate::output_selection::SelectionError::UnreadableTopology);
        };
        // select_for_window validates every entry (including duplicates
        // and rect mismatches) and ranks centre/span/left-top.
        let selected = select_for_window(direction, &source_entry, window, &entries)?;
        let Some(candidate) = selected else {
            return Ok(None);
        };
        if !self.reciprocal_pair_exists(source, &candidate.id, direction) {
            return Ok(None);
        }
        Ok(Some(candidate.id))
    }

    /// Migration directional target over retained FULL topology
    /// (2026-10-09: largest shared edge, then left/top; never window
    /// position). `Err` on unreadable topology; `Ok(None)` on no
    /// candidate or missing reverse touch. No wrap.
    pub(crate) fn migration_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
    ) -> Result<Option<OutputId>, crate::output_selection::SelectionError> {
        use crate::output_selection::select_for_migration;
        let entries = self.live_topology_entries();
        let Some(source_entry) = entries.iter().find(|e| &e.id == source).cloned() else {
            return Err(crate::output_selection::SelectionError::UnreadableTopology);
        };
        let selected = select_for_migration(direction, &source_entry, &entries)?;
        let Some(candidate) = selected else {
            return Ok(None);
        };
        if !self.reciprocal_pair_exists(source, &candidate.id, direction) {
            return Ok(None);
        }
        Ok(Some(candidate.id))
    }

    pub(crate) fn directional_pair_for(
        &self,
        report: &WorldReport,
        source_key: &DomainKey,
        direction: &str,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<DirectionalPair> {
        let _ = report;
        let direction = parse_direction_token(direction)?;
        let source = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == *source_key)?;
        if held_scopes.contains(source_key) {
            return None;
        }
        let target_output = self.core_topology_target(&source_key.output, direction)?;
        self.directional_pair_for_target(source_key, source, direction, &target_output, held_scopes)
    }

    /// Validated reciprocal pair for an explicitly pinned target output
    /// (dispatch-selected via the windowed/migration selectors, or a named
    /// migration endpoint). Verifies the pinned pair still touches
    /// reciprocally in fresh topology without re-ranking any relocated
    /// mover: forward and reverse edge-touch must both hold, reverse
    /// uniqueness is not required. `None` refuses without mutation.
    pub(crate) fn directional_pair_for_target(
        &self,
        source_key: &DomainKey,
        source: &crate::world_engine::StagedDomain,
        direction: Direction,
        target_output: &OutputId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<DirectionalPair> {
        if !self.reciprocal_pair_exists(&source_key.output, target_output, direction) {
            return None;
        }
        let current = self.effective_view(target_output);
        let candidates: Vec<&crate::world_engine::StagedDomain> = self
            .staged
            .domains
            .iter()
            .filter(|d| {
                d.readable && d.key.output == *target_output && !held_scopes.contains(&d.key)
            })
            .collect();
        // Same workspace preferred (directional counterpart), else the
        // target output's current view, else the first readable domain
        // there.
        let target = candidates
            .iter()
            .find(|d| d.key.workspace.0 == source.key.workspace.0)
            .or_else(|| {
                let wanted = current.as_deref()?;
                candidates.iter().find(|d| d.key.workspace.0 == wanted)
            })
            .or_else(|| candidates.first())
            .copied()?;
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let source_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source.bounds,
            gap: source.gap,
            adjacent: BTreeMap::from([(direction, target_output.clone())]),
        };
        let target_domain = OutputDomain {
            id: target_output.clone(),
            workspace: target.key.workspace.clone(),
            bounds: target.bounds,
            gap: target.gap,
            adjacent: BTreeMap::from([(opposite, source_key.output.clone())]),
        };
        Some((
            (source_domain, source_key.clone()),
            (target_domain, target.key.clone()),
            target.outer_gap,
            direction,
        ))
    }

    /// Windowed directional pair for exhausted moves and directional
    /// sends: selects via [`WorldEngine::window_topology_target`] (centre,
    /// span, left/top) then builds the validated reciprocal pair for the
    /// pinned target without re-ranking. `Err` on unreadable topology
    /// (refuse); `Ok(None)` on no candidate, missing reverse touch, or no
    /// readable target domain.
    pub(crate) fn directional_pair_for_window(
        &self,
        report: &WorldReport,
        source_key: &DomainKey,
        direction: Direction,
        window: &crate::geometry::Rect,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<Option<DirectionalPair>, crate::output_selection::SelectionError> {
        let _ = report;
        let Some(source) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == *source_key)
        else {
            return Ok(None);
        };
        if held_scopes.contains(source_key) {
            return Ok(None);
        }
        let selected = self.window_topology_target(&source_key.output, direction, window)?;
        let Some(target_output) = selected else {
            return Ok(None);
        };
        Ok(self.directional_pair_for_target(
            source_key,
            source,
            direction,
            &target_output,
            held_scopes,
        ))
    }
}
