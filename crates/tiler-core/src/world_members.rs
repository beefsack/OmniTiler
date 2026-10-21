//! Core-owned logical membership (D2): stable window-lifetime homing
//! independent of layout sessions.
//!
//! The Engine session is a layout projection only. Floating workspaces
//! release their session (adapter owns native frames), minimized and
//! unreadable facts carry no Engine carriers, and view switches must never
//! yank retained members. This map is the single homing owner: every staged
//! fact assigns through it, every commit rekeys it, and every cleanup prunes
//! it. Transfer provenance only shields actuation lag; it never authorizes
//! homing.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{RestoredWorld, TilingMode, TransferKind, WorldReport};

/// Core-owned per-lifetime logical state: the homed logical workspace plus
/// the exact-lifetime float/minimized marks that survive session release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberState {
    pub(crate) home: DomainKey,
    pub(crate) floating: bool,
    pub(crate) minimized: bool,
    pub(crate) geometry: Option<Rect>,
}

impl WorldEngine {
    /// Single-owner homing read: the retained logical home, remapped off
    /// removed screens onto the displaced survivor.
    pub(crate) fn core_homing(&self, window: &WindowId) -> Option<DomainKey> {
        let home = self.members.get(window)?.home.clone();
        self.rehome_displaced(home)
    }

    /// Adopt bootstrap restoration ingress: persisted rings replace the
    /// (empty, first-report-only) mapping, persisted views seed the retained
    /// views, persisted tiling modes seed the planner-owned modes, and
    /// persisted homing seeds the membership owner. No sessions, transfers,
    /// history, or effects are touched: restored members converge through
    /// the ordinary sweep and settle as retained (never fresh admission),
    /// and history observes the restored views through the ordinary
    /// post-staging path. The logical creation counter advances past any
    /// restored `ws-<n>` ids so later trailing creation never collides.
    pub(crate) fn apply_restored(&mut self, restored: &RestoredWorld) {
        let mut mapping: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for ring in &restored.rings {
            mapping.insert(ring.output.0.clone(), ring.workspaces.clone());
        }
        self.mapping = mapping;
        for view in &restored.views {
            if self
                .mapping
                .get(&view.output.0)
                .is_some_and(|ring| ring.iter().any(|id| id == &view.view))
            {
                self.views.insert(view.output.0.clone(), view.view.clone());
            }
        }
        for entry in &restored.tiling {
            self.tiling.insert(
                DomainKey {
                    output: entry.output.clone(),
                    workspace: WorkspaceId(entry.workspace.clone()),
                },
                if entry.tiled {
                    TilingMode::Tiled
                } else {
                    TilingMode::Floating
                },
            );
        }
        for member in &restored.members {
            self.members.insert(
                member.window.clone(),
                MemberState {
                    home: DomainKey {
                        output: member.output.clone(),
                        workspace: WorkspaceId(member.workspace.clone()),
                    },
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
        }
        let mut floor = self.creation_counter;
        for ring in &restored.rings {
            for id in &ring.workspaces {
                if let Some(number) = id
                    .strip_prefix("ws-")
                    .and_then(|suffix| suffix.parse::<u64>().ok())
                {
                    floor = floor.max(number);
                }
            }
        }
        self.creation_counter = floor;
    }

    /// Whether `id` names a known logical workspace on `output`.
    pub(crate) fn known_workspace(&self, output: &OutputId, id: &str) -> bool {
        self.mapping
            .get(&output.0)
            .is_some_and(|ring| ring.iter().any(|kept| kept == id))
    }

    /// Rekey every member homed at `(from_output, workspace)` onto
    /// `(to_output, workspace)` (migration, swap, displacement, return,
    /// refusal rollback). Tiling modes ride alongside via the existing
    /// [`WorldEngine::carry_tiling`] caller.
    pub(crate) fn rekey_members(&mut self, workspace: &str, from_output: &str, to_output: &str) {
        if from_output == to_output {
            return;
        }
        for state in self.members.values_mut() {
            if state.home.output.0 == from_output && state.home.workspace.0 == workspace {
                state.home.output = OutputId(to_output.to_owned());
            }
        }
    }

    /// Drop members whose home output left the live set without a
    /// displacement record (true cleanup, never silent absence elsewhere).
    pub(crate) fn prune_members_off_live(&mut self, live: &BTreeSet<String>) {
        self.members.retain(|_, state| {
            live.contains(&state.home.output.0) || self.displaced.contains_key(&state.home.output.0)
        });
    }

    /// Commit one window send into the membership owner: the lifetime homes
    /// at the target immediately (mirroring the Engine pair commit for
    /// tiled sends), preserving its float/minimized marks. Sticky facts
    /// never reach here (refused upstream).
    pub(crate) fn commit_send_member(&mut self, window: &WindowId, target: &DomainKey) {
        if let Some(state) = self.members.get_mut(window) {
            state.home = target.clone();
        } else {
            self.members.insert(
                window.clone(),
                MemberState {
                    home: target.clone(),
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
        }
    }

    /// Reconcile the membership owner from the staged observation for the
    /// next report. Explicit closes always drop, even inside incomplete
    /// inventories; implicit absence drops only inside a complete
    /// inventory (never inferred closed across a gap). Unreadable,
    /// minimized, and held facts retain homing. Initial unknown membership
    /// admits to the core view; a truthful `observed_workspace` receipt
    /// naming a known workspace on the observed screen wins over desire.
    /// View switches never yank a resident observed on its retained screen.
    pub(crate) fn reconcile_members(&mut self, report: &WorldReport) {
        for id in &self.staged.closes {
            self.members.remove(id);
        }
        // Live set for pruning implicit closes only when complete.
        let live_outputs: BTreeSet<String> = self
            .staged
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        for (window, fact) in &self.staged.facts.clone() {
            if fact.sticky {
                continue;
            }
            let flight_held = self.retained.iter().any(|r| match &r.target.kind {
                TransferKind::Window(w) => w == window,
                TransferKind::Migration => r.members.iter().any(|m| m == window),
                TransferKind::View => false,
            });
            let observed_known = fact.observed_workspace.as_deref().and_then(|obs| {
                if self.known_workspace(&fact.output, obs) {
                    Some(DomainKey {
                        output: fact.output.clone(),
                        workspace: WorkspaceId(obs.to_owned()),
                    })
                } else if fact.floating {
                    // Floating logical crossing preserves the physical
                    // frame: adopt the receipt on its owning screen.
                    self.owning_output(obs).map(|output| DomainKey {
                        output,
                        workspace: WorkspaceId(obs.to_owned()),
                    })
                } else {
                    None
                }
            });
            let home_output = self
                .members
                .get(window)
                .map(|state| state.home.output.clone());
            // Precompute adoption view before the mutable borrow.
            let adopt = if fact.observed_workspace.is_none()
                && home_output
                    .as_ref()
                    .is_some_and(|home| home != &fact.output)
                && !flight_held
            {
                self.effective_view(&fact.output)
                    .filter(|view| self.known_workspace(&fact.output, view))
            } else {
                None
            };
            if let Some(state) = self.members.get_mut(window) {
                if flight_held {
                    // Outstanding membership flights shield homing: delayed
                    // arrival stays at the committed target until
                    // settlement proves otherwise. Only explicit float and
                    // minimized marks refresh; homing never flaps back.
                    state.floating = fact.floating;
                    state.minimized = fact.minimized;
                    state.geometry = fact.rect;
                    continue;
                }
                if let Some(known) = observed_known {
                    state.home = known;
                } else if let Some(view) = adopt {
                    state.home = DomainKey {
                        output: fact.output.clone(),
                        workspace: WorkspaceId(view),
                    };
                }
                // Contradictory receipts naming an unknown workspace hold:
                // keep the retained home, never fight newer input.
                state.floating = fact.floating;
                state.minimized = fact.minimized;
                state.geometry = fact.rect;
            } else {
                // Newcomer admission: truthful known receipt wins, else the
                // screen's effective view (never silent absence).
                let home = observed_known.or_else(|| {
                    self.effective_view(&fact.output).map(|view| DomainKey {
                        output: fact.output.clone(),
                        workspace: WorkspaceId(view),
                    })
                });
                if let Some(home) = home {
                    self.members.insert(
                        window.clone(),
                        MemberState {
                            home,
                            floating: fact.floating,
                            minimized: fact.minimized,
                            geometry: fact.rect,
                        },
                    );
                }
            }
        }
        if report.inventory_complete {
            let present: BTreeSet<WindowId> = self.staged.facts.keys().cloned().collect();
            // Outstanding transfer members stay across the gap: absent
            // without a close never infers closure while a flight names
            // them (arrival may lag one report behind the commit).
            let mut flight_members: BTreeSet<WindowId> = BTreeSet::new();
            for retained in &self.retained {
                match &retained.target.kind {
                    TransferKind::Window(window) => {
                        flight_members.insert(window.clone());
                    }
                    TransferKind::Migration => {
                        for member in &retained.members {
                            flight_members.insert(member.clone());
                        }
                    }
                    TransferKind::View => {}
                }
            }
            self.members.retain(|window, state| {
                present.contains(window)
                    || !live_outputs.contains(&state.home.output.0)
                    || flight_members.contains(window)
                    || self.floating_scopes.contains(&state.home)
            });
        }
        self.prune_members_off_live(&live_outputs);
    }

    /// Read-only hostability probe for [`WorldEngine::relocate_session_for_move`]:
    /// whether the relocate would succeed without mutating. Empty or absent
    /// sources always host (nothing stateful to carry); otherwise the target
    /// screen must exist with a valid work area/inset, the destination slot
    /// must be free, and the domain relocate must validate on a clone.
    pub(crate) fn can_relocate_session_for_move(
        &self,
        source: &DomainKey,
        target: &DomainKey,
    ) -> bool {
        if source == target {
            return true;
        }
        let Some(session) = self.engine.session(source) else {
            return true;
        };
        let nonempty = !session.snapshot().windows.is_empty()
            || session.exception_count() > 0
            || !session.minimized_entries().is_empty();
        if !nonempty {
            return true;
        }
        let Some(screen) = self
            .staged
            .screens
            .iter()
            .find(|screen| screen.output == target.output)
        else {
            return false;
        };
        let Some(area) = screen.work_area else {
            return false;
        };
        let Ok(bounds) = crate::geometry::inset_bounds(area, screen.outer_gap) else {
            return false;
        };
        if self.engine.session(target).is_some() {
            return false;
        }
        let mut moved = session.clone();
        moved.relocate_domain(source, target, bounds, screen.gap)
    }

    /// Relocate one retained Engine session across outputs, preserving
    /// tree/shares/focus/exceptions/minimized slots (hotplug displacement,
    /// return, and migration-refusal rollback share this path). Returns
    /// false without mutation when there is nothing stateful to carry or
    /// the target geometry cannot host it.
    pub(crate) fn relocate_session_for_move(
        &mut self,
        source: &DomainKey,
        target: &DomainKey,
    ) -> bool {
        if source == target {
            return true;
        }
        let Some(session) = self.engine.session(source).cloned() else {
            return true;
        };
        let nonempty = !session.snapshot().windows.is_empty()
            || session.exception_count() > 0
            || !session.minimized_entries().is_empty();
        if !nonempty {
            self.engine.remove(source);
            return true;
        }
        let Some(screen) = self
            .staged
            .screens
            .iter()
            .find(|screen| screen.output == target.output)
        else {
            return false;
        };
        let Some(area) = screen.work_area else {
            return false;
        };
        let Ok(bounds) = crate::geometry::inset_bounds(area, screen.outer_gap) else {
            return false;
        };
        if self.engine.session(target).is_some() {
            return false;
        }
        let mut moved = session;
        if !moved.relocate_domain(source, target, bounds, screen.gap) {
            return false;
        }
        let outer = screen.outer_gap;
        self.engine.remove(source);
        self.engine.insert_raw(target.clone(), moved, outer);
        true
    }
}
