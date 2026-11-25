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

use crate::world_transaction::WorldTransaction;
use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::workspace_scope::DisplacedEntry;
use crate::world_engine::WorldEngine;
use crate::world_report::TransferKind;

/// Core-owned per-lifetime logical state: the homed logical workspace plus
/// the exact-lifetime float/minimized marks that survive session release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberState {
    pub(crate) home: DomainKey,
    pub(crate) floating: bool,
    pub(crate) minimized: bool,
    pub(crate) geometry: Option<Rect>,
}

/// Rehome a retained key off a removed screen onto its displaced survivor,
/// keeping the logical id. `None` when the key is live or no live survivor
/// hosts it.
pub(crate) fn rehome_displaced_in(
    mapping: &BTreeMap<String, Vec<String>>,
    displaced: &BTreeMap<String, DisplacedEntry>,
    key: DomainKey,
) -> Option<DomainKey> {
    if mapping.contains_key(&key.output.0) {
        return Some(key);
    }
    let dest = displaced.get(&key.output.0)?.dest.clone();
    if !mapping.contains_key(&dest) {
        return None;
    }
    Some(DomainKey {
        output: OutputId(dest),
        workspace: key.workspace,
    })
}

/// Single-owner homing read: the retained logical home, remapped off removed
/// screens onto the displaced survivor.
pub(crate) fn core_homing_in(
    members: &BTreeMap<WindowId, MemberState>,
    mapping: &BTreeMap<String, Vec<String>>,
    displaced: &BTreeMap<String, DisplacedEntry>,
    window: &WindowId,
) -> Option<DomainKey> {
    let home = members.get(window)?.home.clone();
    rehome_displaced_in(mapping, displaced, home)
}

impl WorldTransaction<'_> {
    /// Single-owner homing read: the retained logical home, remapped off
    /// removed screens onto the displaced survivor.
    pub(crate) fn core_homing(&self, window: &WindowId) -> Option<DomainKey> {
        self.world.workspace.core_homing(window)
    }

    /// Whether `id` names a known logical workspace on `output`.
    pub(crate) fn known_workspace(&self, output: &OutputId, id: &str) -> bool {
        self.world
            .workspace
            .mapping
            .get(&output.0)
            .is_some_and(|ring| ring.iter().any(|kept| kept == id))
    }

    /// Rekey every non-sticky member homed at `(from_output, workspace)`
    /// onto `(to_output, workspace)` (migration, swap, displacement,
    /// return, refusal rollback). Tiling modes ride alongside via the
    /// existing [`WorldEngine::carry_tiling`] caller.
    ///
    /// Composed-sticky lifetimes are exempt: their membership home is the
    /// all-desktop anchor and must not follow swap/migration relocations
    /// (the physical window stays put while logical units move around it).
    /// Displaced-output reads still remap onto the survivor through
    /// [`rehome_displaced_in`]/[`core_homing_in`], so retaining the
    /// recorded home stays truthful, and [`Self::prune_members_off_live`]
    /// keeps it while a displacement record exists. Sticky truth comes
    /// from the staged derived lane, never the raw native fact: hosts
    /// without native sticky support report a null fact while core's
    /// retained intent still proves the lifetime.
    pub(crate) fn rekey_members(&mut self, workspace: &str, from_output: &str, to_output: &str) {
        if from_output == to_output {
            return;
        }
        // Precompute sticky member keys: the derived map must be read
        // while `members` is only borrowed immutably, and it is the
        // authority for composed sticky (raw facts stay null there).
        let sticky: BTreeSet<WindowId> = self
            .world
            .workspace
            .members
            .keys()
            .filter(|window| {
                self.world
                    .staged
                    .derived
                    .get(*window)
                    .is_some_and(|derived| derived.sticky)
            })
            .cloned()
            .collect();
        for (window, state) in self.world.workspace.members.iter_mut() {
            if sticky.contains(window) {
                continue;
            }
            if state.home.output.0 == from_output && state.home.workspace.0 == workspace {
                state.home.output = OutputId(to_output.to_owned());
            }
        }
    }

    /// Drop members whose home output left the live set without a
    /// displacement record (true cleanup, never silent absence elsewhere).
    pub(crate) fn prune_members_off_live(&mut self, live: &BTreeSet<String>) {
        self.world.workspace.members.retain(|_, state| {
            live.contains(&state.home.output.0)
                || self
                    .world
                    .workspace
                    .displaced
                    .contains_key(&state.home.output.0)
        });
    }

    /// Commit one window send into the membership owner: the lifetime homes
    /// at the target immediately (mirroring the Engine pair commit for
    /// tiled sends), preserving its float/minimized marks. Sticky facts
    /// never reach here (refused upstream).
    pub(crate) fn commit_send_member(&mut self, window: &WindowId, target: &DomainKey) {
        // A minimized member's restore slot stores the source workspace's
        // tree; restoring it there after membership moved would regress the
        // committed home. Forget it on the move: a later restore admits
        // fresh into the target (D23 option B).
        let mut stale_slot = None;
        if let Some(state) = self.world.workspace.members.get_mut(window) {
            if state.minimized {
                stale_slot = Some(state.home.clone());
            }
            state.home = target.clone();
        } else {
            self.world.workspace.members.insert(
                window.clone(),
                MemberState {
                    home: target.clone(),
                    floating: false,
                    minimized: false,
                    geometry: None,
                },
            );
        }
        if let Some(source) = stale_slot {
            self.world.layout.forget_minimized_slot(&source, window);
        }
    }

    /// Reconcile the membership owner from the staged observation for the
    /// next report. Explicit closes always drop, even inside incomplete
    /// inventories; implicit absence drops only inside a complete
    /// inventory (never inferred closed across a gap). Unreadable,
    /// minimized, and held facts retain homing. Narrowly held rows count
    /// as live presence (never a departure): a complete malformed or
    /// conflicting row keeps its member, so a later recovery reuses the
    /// same lifetime instead of reminting. Known and own-process safety
    /// exclusions drop membership outright, complete and incomplete
    /// alike (never a native close, never a minimize slot: the sweep
    /// converges them as closes). Initial unknown membership admits to
    /// the core view; a truthful `observed_workspace` receipt naming a
    /// known workspace on the observed screen wins over desire.
    /// View switches never yank a resident observed on its retained screen.
    pub(crate) fn reconcile_members(&mut self) {
        for id in &self.world.staged.closes {
            self.world.workspace.members.remove(id);
        }
        // Safety exclusions never enter or stay: drop even inside
        // incomplete inventories (no stale member, no stale tiled mark).
        for id in self.world.staged.excluded.clone() {
            self.world.workspace.members.remove(&id);
        }
        // Live set for pruning implicit closes only when complete.
        let live_outputs: BTreeSet<String> = self
            .world
            .staged
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        for (window, fact) in &self.world.staged.facts.clone() {
            if fact.sticky == Some(true) {
                continue;
            }
            let derived_member = self.derived_for(window);
            if derived_member.excluded || derived_member.own_excluded {
                continue;
            }
            let flight_held = self
                .world
                .settlement
                .retained
                .iter()
                .any(|r| match &r.target.kind {
                    TransferKind::Window(w) => w == window,
                    TransferKind::Migration => r.members.iter().any(|m| m == window),
                    TransferKind::View => false,
                });
            // Effective receipt: native-derived under the capability
            // (unknown/conflicting holds), adapter logical otherwise.
            let receipt = self.effective_logical_receipt(fact);
            let observed_known = receipt.as_deref().and_then(|obs| {
                let output = fact.output.as_ref()?;
                if self.known_workspace(output, obs) {
                    Some(DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(obs.to_owned()),
                    })
                } else if derived_member.floating {
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
                .world
                .workspace
                .members
                .get(window)
                .map(|state| state.home.output.clone());
            // Precompute adoption view before the mutable borrow.
            // Native-free only: unknown logical membership adopts the
            // screen's effective view. With the capability authoritative,
            // unknown native membership keeps the retained home with no
            // view adoption and never collapses (write authority is
            // fenced at publish; keeping the home invents no proof).
            // Unknown output never adopts: no fabricated homing.
            let adopt = if !self.connection.host_owned_workspace_domains()
                && receipt.is_none()
                && home_output
                    .as_ref()
                    .is_some_and(|home| Some(home) != fact.output.as_ref())
                && !flight_held
            {
                fact.output.as_ref().and_then(|output| {
                    self.effective_view(output)
                        .filter(|view| self.known_workspace(output, view))
                })
            } else {
                None
            };
            if let Some(state) = self.world.workspace.members.get_mut(window) {
                if flight_held {
                    // Outstanding membership flights shield homing: delayed
                    // arrival stays at the committed target until
                    // settlement proves otherwise. Only explicit float and
                    // hidden-lane (minimized or host-hidden) marks refresh;
                    // homing never flaps back.
                    state.floating = derived_member.floating;
                    state.minimized = derived_member.hidden;
                    state.geometry = fact.rect;
                    continue;
                }
                if let Some(known) = observed_known {
                    state.home = known;
                } else if let Some(view) = adopt
                    && let Some(output) = fact.output.clone()
                {
                    state.home = DomainKey {
                        output,
                        workspace: WorkspaceId(view),
                    };
                }
                // Contradictory receipts naming an unknown workspace hold:
                // keep the retained home, never fight newer input.
                state.floating = derived_member.floating;
                state.minimized = derived_member.hidden;
                state.geometry = fact.rect;
            } else {
                // Newcomer admission: truthful known receipt wins, else the
                // screen's effective view (native-free only; never silent
                // absence). With the capability authoritative, unknown
                // native membership stays unhomed: newcomers publish no
                // row and authorize no writes until native evidence
                // arrives.
                let home = observed_known.or_else(|| {
                    if self.connection.host_owned_workspace_domains() {
                        None
                    } else {
                        fact.output.as_ref().and_then(|output| {
                            self.effective_view(output).map(|view| DomainKey {
                                output: output.clone(),
                                workspace: WorkspaceId(view),
                            })
                        })
                    }
                });
                if let Some(home) = home {
                    self.world.workspace.members.insert(
                        window.clone(),
                        MemberState {
                            home,
                            floating: derived_member.floating,
                            minimized: derived_member.hidden,
                            geometry: fact.rect,
                        },
                    );
                }
            }
        }
        if self.world.staged.observation.inventory_complete {
            let mut present: BTreeSet<WindowId> = self.world.staged.facts.keys().cloned().collect();
            // Narrowly held rows are live presence without usable facts:
            // they never prune a retained member.
            for held in &self.world.staged.held_windows {
                present.insert(held.window.clone());
            }
            // Outstanding transfer members stay across the gap: absent
            // without a close never infers closure while a flight names
            // them (arrival may lag one report behind the commit).
            let mut flight_members: BTreeSet<WindowId> = BTreeSet::new();
            for retained in &self.world.settlement.retained {
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
            self.world.workspace.members.retain(|window, state| {
                present.contains(window)
                    || !live_outputs.contains(&state.home.output.0)
                    || flight_members.contains(window)
                    || self.world.floating_scopes.contains(&state.home)
            });
        }
        self.prune_members_off_live(&live_outputs);
    }

    /// D23 option B: forget the restore slot on the first confirmed
    /// per-window minimized rehome (receipt-proven staged home differs
    /// from a retained slot home). Unknown receipts, in-flight window
    /// transfers, migrations, displaced sources, and closes never
    /// confirm. Applied post-sweep, so same-turn minimize+rehome is
    /// covered too. Revision-neutral.
    pub(crate) fn confirmed_minimized_rehomes(&self) -> Vec<(DomainKey, WindowId)> {
        if !self.world.staged.observation.inventory_complete {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (window, fact) in &self.world.staged.facts {
            if fact.minimized != Some(true) {
                continue;
            }
            if self.world.staged.closes.contains(window) {
                continue;
            }
            let Some(output) = fact.output.clone() else {
                continue;
            };
            let Some(receipt) = self.effective_logical_receipt(fact) else {
                continue;
            };
            if !self.known_workspace(&output, &receipt) {
                continue;
            }
            let Some(dest) = self.world.staged.homing.get(window) else {
                continue;
            };
            if dest.output != output || dest.workspace.0 != receipt {
                continue;
            }
            if self.world.settlement.retained.iter().any(
                |retained| matches!(&retained.target.kind, TransferKind::Window(w) if w == window),
            ) {
                continue;
            }
            for key in self.world.layout.domain_keys() {
                if key == *dest {
                    continue;
                }
                if self.world.workspace.displaced.contains_key(&key.output.0) {
                    continue;
                }
                if !self.world.layout.has_minimized_slot(&key, window) {
                    continue;
                }
                if self.world.settlement.retained.iter().any(|retained| {
                    matches!(retained.target.kind, TransferKind::Migration)
                        && (retained.source_workspace == key.workspace.0
                            && retained.source_output == key.output
                            || retained.target.target_workspace == dest.workspace.0
                                && retained.target.target_output == dest.output)
                }) {
                    continue;
                }
                out.push((key, window.clone()));
            }
        }
        out
    }

    /// Read-only hostability probe for [`WorldEngine::relocate_session_for_move`]:
    /// whether the move relocation would succeed without mutating the layout.
    /// Absent or empty sources always host (nothing stateful to carry);
    /// otherwise the target screen must exist with a valid work area/inset,
    /// the destination must be unoccupied, and the retained domain must
    /// validate at the requested geometry.
    pub(crate) fn can_relocate_session_for_move(
        &self,
        source: &DomainKey,
        target: &DomainKey,
    ) -> bool {
        if source == target {
            return true;
        }
        if !self.world.layout.contains_domain(source) {
            return true;
        }
        let nonempty = self.world.layout.tiled_count(source) > 0
            || self.world.layout.exception_count(source) > 0
            || self.world.layout.minimized_count(source) > 0;
        if !nonempty {
            return true;
        }
        let Some(screen) = self
            .world
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
        let Some(outer_gap) =
            WorldEngine::scale_once(self.connection.outer_gap_setting(), screen.scale)
        else {
            return false;
        };
        let Ok(bounds) = crate::geometry::inset_bounds(area, outer_gap) else {
            return false;
        };
        if self.world.layout.contains_domain(target) {
            return false;
        }
        let Some(gap) = WorldEngine::scale_once(self.connection.gap_setting(), screen.scale) else {
            return false;
        };
        self.world
            .layout
            .can_relocate_domain(source, target, bounds, gap)
    }

    /// Move one retained layout domain across outputs, preserving its tree,
    /// shares, focus, focus history, exceptions, restore slots and accepted
    /// revision (hotplug displacement, return, and migration-refusal rollback
    /// share this path). Returns false without mutation when the target
    /// geometry cannot host the source or the exact-domain relocation refuses.
    /// An empty source is simply dropped; an absent source is a no-op success.
    pub(crate) fn relocate_session_for_move(
        &mut self,
        source: &DomainKey,
        target: &DomainKey,
    ) -> bool {
        if source == target {
            return true;
        }
        if !self.world.layout.contains_domain(source) {
            return true;
        }
        let nonempty = self.world.layout.tiled_count(source) > 0
            || self.world.layout.exception_count(source) > 0
            || self.world.layout.minimized_count(source) > 0;
        if !nonempty {
            self.world.layout.drop_domain(source);
            return true;
        }
        let Some(screen) = self
            .world
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
        let Some(outer_gap) =
            WorldEngine::scale_once(self.connection.outer_gap_setting(), screen.scale)
        else {
            return false;
        };
        let Ok(bounds) = crate::geometry::inset_bounds(area, outer_gap) else {
            return false;
        };
        if self.world.layout.contains_domain(target) {
            return false;
        }
        let Some(gap) = WorldEngine::scale_once(self.connection.gap_setting(), screen.scale) else {
            return false;
        };
        self.world
            .layout
            .relocate_domain(source, target, bounds, gap, outer_gap)
    }
}
