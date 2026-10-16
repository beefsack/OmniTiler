use super::*;

impl ManagedWorkspaces {
    /// Exchange two workspace views across outputs (global-unique
    /// visible-elsewhere swap primitive, item 12 stage B).
    ///
    /// Implements the stage A core contract
    /// ([`tiler_core::workspace_scope::apply_swap_assignments`]) natively:
    /// both ids leave every scope, then the target appends to the active
    /// output's scope end and the active current appends to the other
    /// output's scope end (KDE assignment order; ordinals churn by design).
    /// The plan derives from observed current views, never from assumed
    /// entry homing: even when the target is listed (owned) on the
    /// requesting output but shown on the other output, both ids are
    /// stripped everywhere and re-appended per the plan. Members,
    /// remembered focus (`last_focus`), and entry tokens travel with the
    /// entries; per-member output locations rekey by id to the new home
    /// output while workspace membership and hidden state are preserved.
    /// Same-id duplicates (shared-convergence residue) merge members
    /// losslessly into the traveling entry, keeping the shown output's
    /// remembered focus; session tiled state is keyed by workspace id and
    /// travels unchanged. Both current views land via explicit activation,
    /// so views and assignments always reflect the plan.
    ///
    /// Sticky all-desktop members mirror the migration rule: they stay homed
    /// on their output and never ride a traveling entry. Sticky keys are
    /// stripped from both traveling entries (a sticky remembered-focus entry
    /// clears rather than carries) and re-homed to the incoming entry on
    /// their own output with hidden state preserved; sticky locations
    /// outside the two participating outputs stay untouched (the migration
    /// no-refill precedent).
    ///
    /// Returns false and changes nothing when either output is unknown,
    /// either id is absent everywhere, or both ids address the same output.
    /// Local per-output history (`previous`/`baseline`) is untouched:
    /// scoped modes record into the scope owner, and local mode never swaps.
    pub fn swap_entries(
        &mut self,
        a_output: &str,
        a_id: &str,
        b_output: &str,
        b_id: &str,
        sticky: &BTreeSet<WindowKey>,
    ) -> bool {
        if a_output == b_output {
            return false;
        }
        if !self.outputs.contains_key(a_output) || !self.outputs.contains_key(b_output) {
            return false;
        }
        if !self
            .outputs
            .values()
            .any(|s| s.order.iter().any(|e| e.id == a_id))
            || !self
                .outputs
                .values()
                .any(|s| s.order.iter().any(|e| e.id == b_id))
        {
            return false;
        }
        // Strip both ids from every scope, collecting the traveling entries.
        // Same-id duplicates merge members losslessly; remembered focus
        // prefers the entry homed on the output that showed the id.
        let mut traveling_a: Option<WorkspaceEntry> = None;
        let mut traveling_b: Option<WorkspaceEntry> = None;
        for (output_key, state) in self.outputs.iter_mut() {
            let mut kept = Vec::with_capacity(state.order.len());
            for mut entry in std::mem::take(&mut state.order) {
                let slot = if entry.id == a_id {
                    &mut traveling_a
                } else if entry.id == b_id {
                    &mut traveling_b
                } else {
                    kept.push(entry);
                    continue;
                };
                let shown_here = output_key == (if entry.id == a_id { a_output } else { b_output });
                match slot.take() {
                    None => *slot = Some(entry),
                    Some(mut base) => {
                        if shown_here {
                            for key in base.members {
                                entry.members.insert(key);
                            }
                            if entry.last_focus.is_none() {
                                entry.last_focus = base.last_focus;
                            }
                            *slot = Some(entry);
                        } else {
                            for key in entry.members {
                                base.members.insert(key);
                            }
                            if base.last_focus.is_none() {
                                base.last_focus = entry.last_focus;
                            }
                            *slot = Some(base);
                        }
                    }
                }
            }
            state.order = kept;
            state.active = state.active.min(state.order.len().saturating_sub(1));
        }
        let Some(mut entry_a) = traveling_a else {
            return false;
        };
        let Some(mut entry_b) = traveling_b else {
            return false;
        };
        for entry in [&mut entry_a, &mut entry_b] {
            for key in sticky.iter() {
                entry.members.remove(key);
                if entry.last_focus.as_ref() == Some(key) {
                    entry.last_focus = None;
                }
            }
        }
        // Rekey by id, never by assumed homing: every non-sticky member of a
        // swapped id moves to the destination output; sticky members keep
        // their output and follow the swap only across the two participating
        // outputs.
        for (key, loc) in self.membership.iter_mut() {
            if loc.workspace == a_id {
                if sticky.contains(key) {
                    if loc.output == a_output {
                        loc.workspace = b_id.to_owned();
                    }
                } else {
                    loc.output = b_output.to_owned();
                }
            } else if loc.workspace == b_id {
                if sticky.contains(key) {
                    if loc.output == b_output {
                        loc.workspace = a_id.to_owned();
                    }
                } else {
                    loc.output = a_output.to_owned();
                }
            }
        }
        // Append per the contract: target to the active scope end, active
        // current to the other scope end.
        if let Some(state) = self.outputs.get_mut(a_output) {
            state.order.push(entry_b);
        }
        if let Some(state) = self.outputs.get_mut(b_output) {
            state.order.push(entry_a);
        }
        // Sticky re-home into the incoming entries' member sets (the
        // migration rule): a sticky key whose location now names the
        // incoming entry joins that entry's set so location and membership
        // agree and the sticky window stays visible on its home output.
        for output in [a_output, b_output] {
            let Some(state) = self.outputs.get_mut(output) else {
                continue;
            };
            for entry in state.order.iter_mut() {
                for key in sticky.iter() {
                    if self
                        .membership
                        .get(key)
                        .is_some_and(|loc| loc.output == output && loc.workspace == entry.id)
                    {
                        entry.members.insert(key.clone());
                    }
                }
            }
        }
        let a_ok = self.activate(a_output, b_id);
        let b_ok = self.activate(b_output, a_id);
        // Infallible by the pre-checks plus the appends above: both outputs
        // are known, both ids were present, and each swapped id was just
        // appended to its destination scope end. A false here would mean the
        // entries already exchanged while the views did not land (partial
        // without history); no rollback, the debug assert owns the invariant.
        debug_assert!(a_ok && b_ok, "swap appends land activation");
        a_ok && b_ok
    }

    /// Move whole workspaces from origin onto a caller-selected survivor.
    /// The caller chooses the survivor (via the core chooser when
    /// disconnect-time geometry is available). Returns false when origin or
    /// destination is missing, or both are the same.
    pub fn displace_output_to(&mut self, origin: &str, dest: &str) -> bool {
        if origin == dest || !self.outputs.contains_key(origin) || !self.outputs.contains_key(dest)
        {
            return false;
        }
        let ids: Vec<String> = self
            .outputs
            .get(origin)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            self.outputs.remove(origin);
            // A disconnected output's history is discarded with the output.
            self.discard_output_history(origin);
            self.validate_history(dest);
            return true;
        }
        let moving: Vec<WorkspaceEntry> = self
            .outputs
            .get_mut(origin)
            .map(|s| std::mem::take(&mut s.order))
            .unwrap_or_default();
        self.outputs.remove(origin);
        for window in self.membership.values_mut() {
            if window.output == origin && ids.contains(&window.workspace) {
                window.output = dest.to_owned();
            }
        }
        if let Some(state) = self.outputs.get_mut(dest) {
            state.order.extend(moving);
        }
        self.displaced.insert(
            origin.to_owned(),
            DisplacedRecord {
                workspace_ids: ids,
                dest_key: dest.to_owned(),
            },
        );
        // Disconnect drops BOTH the previous entry and the observed baseline
        // for the removed origin; the survivor's previous stays valid only
        // while still in its scope (movement to another output clears like
        // removal, so toggle is a no-op until the next recorded change).
        self.discard_output_history(origin);
        self.validate_history(dest);
        true
    }

    /// Return displaced workspaces to the exact origin key with current
    /// contents. Creates the origin without baseline empties so no duplicate
    /// trailing/min entries appear; active points at the first returning
    /// workspace.
    pub fn reconnect_output(&mut self, origin: &str) -> bool {
        let Some(record) = self.displaced.get(origin).cloned() else {
            return false;
        };
        let Some(dest_state) = self.outputs.get_mut(&record.dest_key) else {
            return false;
        };
        let mut returning = Vec::new();
        dest_state.order.retain(|e| {
            if record.workspace_ids.contains(&e.id) {
                returning.push(e.clone());
                false
            } else {
                true
            }
        });
        if dest_state.active >= dest_state.order.len() {
            dest_state.active = dest_state.order.len().saturating_sub(1);
        }
        for window in self.membership.values_mut() {
            if window.output == record.dest_key && record.workspace_ids.contains(&window.workspace)
            {
                window.output = origin.to_owned();
            }
        }
        if self.outputs.contains_key(origin) {
            if let Some(state) = self.outputs.get_mut(origin) {
                for entry in returning {
                    if !state.order.iter().any(|e| e.id == entry.id) {
                        state.order.push(entry);
                    }
                }
            }
        } else {
            self.next_output += 1;
            let token = format!("o{}", self.next_output);
            self.outputs.insert(
                origin.to_owned(),
                OutputState {
                    key: origin.to_owned(),
                    token,
                    order: returning,
                    active: 0,
                },
            );
        }
        // Returning workspaces keep their stored session state; ids without
        // one seed from the current default (never a stale foreign value).
        for id in &record.workspace_ids {
            self.seed_mode(id);
        }
        self.displaced.remove(origin);
        // Reconnect selection never consults or restores history: clear any
        // stale previous for the origin and prime a fresh baseline at the
        // returning active view. The next observed displacement/return change
        // records from there. The survivor's history validates against its
        // shrunken scope (a moved previous D clears on L like removal).
        self.previous.remove(origin);
        if let Some(active) = self.active_id(origin) {
            self.baseline.insert(origin.to_owned(), active);
        } else {
            self.baseline.remove(origin);
        }
        self.validate_history(&record.dest_key);
        self.validate_history(origin);
        true
    }

    /// Shared-mode disconnect without project displacement (item 12 stage C,
    /// KDE parity: `reconcileOutputDisplacement` returns early for shared).
    ///
    /// Drops the removed output's entry with no displaced record and no
    /// return mapping: the surviving destination first adopts any missing
    /// origin ids, then origin members re-home under the same logical id
    /// (hidden state preserved; remembered focus stays with the surviving
    /// entries). The origin's local
    /// previous/baseline discards with the output; the destination validates.
    /// Returns the origin's id list for stale Engine-session pruning.
    /// Returns an empty list and changes nothing when either output is
    /// unknown or both are the same.
    pub fn drop_output_merge_to(&mut self, origin: &str, dest: &str) -> Vec<String> {
        if origin == dest || !self.outputs.contains_key(origin) || !self.outputs.contains_key(dest)
        {
            return Vec::new();
        }
        let ids: Vec<String> = self
            .outputs
            .get(origin)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default();
        let mut moving: BTreeMap<String, Vec<WindowKey>> = BTreeMap::new();
        for (key, loc) in &self.membership {
            if loc.output == origin {
                moving
                    .entry(loc.workspace.clone())
                    .or_default()
                    .push(key.clone());
            }
        }
        // Converge the destination onto the origin's id set first so no
        // same-logical-id membership strands on an unlisted id: transiently
        // divergent shared unions converge here (empty entries, fresh
        // tokens), never as orphans.
        for ws in &ids {
            if self
                .outputs
                .get(dest)
                .is_none_or(|s| s.order.iter().all(|e| e.id != *ws))
            {
                self.ensure_workspace_id(dest, ws);
            }
        }
        self.outputs.remove(origin);
        for (ws, keys) in &moving {
            let listed = self
                .outputs
                .get(dest)
                .is_some_and(|s| s.order.iter().any(|e| e.id == *ws));
            if !listed {
                continue;
            }
            if let Some(state) = self.outputs.get_mut(dest)
                && let Some(entry) = state.order.iter_mut().find(|e| e.id == *ws)
            {
                for key in keys {
                    entry.members.insert(key.clone());
                    if let Some(loc) = self.membership.get_mut(key) {
                        loc.output = dest.to_owned();
                    }
                }
            }
        }
        self.discard_output_history(origin);
        self.validate_history(dest);
        ids
    }

    /// Shared-mode reconnect convergence (item 12 stage C): recreate the
    /// returning origin with exactly the live shared ids (fresh per-output
    /// tokens, default-seeded session modes, empty members) and activate the
    /// given view when listed. Unlike `ensure_output` this never mints
    /// baseline empties, so no non-shared residue pollutes the shared ring.
    /// A missing active falls back to the first id. The origin baseline
    /// primes at the returning view without recording: reconnect selection
    /// never consults history. Existing origins converge missing ids and
    /// re-activate (degenerate guard; shared disconnects always drop).
    pub fn insert_shared_output(&mut self, origin: &str, ids: &[String], active_id: &str) {
        if !self.outputs.contains_key(origin) {
            self.next_output += 1;
            let token = format!("o{}", self.next_output);
            let mut order = Vec::new();
            for id in ids {
                self.next_ws += 1;
                let token = format!("ws{}", self.next_ws);
                self.seed_mode(id);
                order.push(WorkspaceEntry {
                    id: id.clone(),
                    token,
                    members: BTreeSet::new(),
                    last_focus: None,
                });
            }
            let active = ids
                .iter()
                .position(|id| id == active_id)
                .unwrap_or(0)
                .min(order.len().saturating_sub(1));
            self.outputs.insert(
                origin.to_owned(),
                OutputState {
                    key: origin.to_owned(),
                    token,
                    order,
                    active,
                },
            );
        } else {
            for id in ids {
                if self
                    .outputs
                    .get(origin)
                    .is_some_and(|s| s.order.iter().any(|e| e.id == *id))
                {
                    continue;
                }
                self.next_ws += 1;
                let token = format!("ws{}", self.next_ws);
                self.seed_mode(id);
                if let Some(state) = self.outputs.get_mut(origin) {
                    state.order.push(WorkspaceEntry {
                        id: id.clone(),
                        token,
                        members: BTreeSet::new(),
                        last_focus: None,
                    });
                }
            }
            self.activate(origin, active_id);
        }
        if let Some(active) = self.active_id(origin) {
            self.baseline.insert(origin.to_owned(), active);
        }
        self.validate_history(origin);
    }

    /// Move the source output's whole active workspace onto the target output
    /// (item 14 portable reassignment; the native runtime verifies arrival
    /// separately before publication). The entry keeps its stable id, token, members,
    /// remembered focus and session tiling state; it is inserted immediately
    /// after the target's current workspace and shown (target active points
    /// at it; the previous target workspace stays listed and hidden).
    ///
    /// The source refills from the configured G-37 mode using the
    /// pre-mutation item-1.2 per-output previous id: the shared
    /// [`tiler_core::output_selection::select_migration_source_refill`]
    /// selector runs over the remaining source scope with the migrated id
    /// excluded (surviving empties qualify; missing, removed, migrated or
    /// out-of-scope entries fall back to last remaining). A still-scoped
    /// live source current view is preserved: when the selector returns an
    /// id, the source activates it; when nothing remains the source keeps an
    /// empty order for ordinary lifecycle to converge.
    ///
    /// Scoped history invalidation (items 1.3/1.5) applies to both outputs;
    /// hotplug-return associations drop only the explicitly moved id, so the
    /// migrated workspace loses auto return while sibling associations
    /// survive. Never simulates the move as N per-window sends and never
    /// moves the destination workspace back to the source. Returns `None`
    /// (changing nothing) when the outputs are the same or unknown, or the
    /// source holds no active workspace.
    pub fn migrate_workspace(
        &mut self,
        source_output: &str,
        target_output: &str,
        refill: tiler_core::output_selection::MigrationSourceRefill,
    ) -> Option<MigrationOutcome> {
        self.migrate_workspace_excluding(source_output, target_output, refill, &BTreeSet::new())
    }

    /// Filtered whole-workspace reassignment for the native migration runtime:
    /// sticky all-desktops members stay homed on the source output and never
    /// ride the moved entry. Sticky keys are removed from the moved entry's
    /// member set before the insert and re-homed to the selected source
    /// refill (when one exists); their `MemberLoc.hidden` state is preserved
    /// and their remembered-focus entries travel only with non-sticky focus.
    /// When the source keeps no workspace (`refill` is `None`) sticky members
    /// keep their existing locations untouched so no dangling re-home is
    /// invented; the caller converges ordinary lifecycle before verification.
    /// The unfiltered [`ManagedWorkspaces::migrate_workspace`] wrapper passes
    /// an empty sticky set and preserves its exact contract for existing
    /// callers.
    pub fn migrate_workspace_excluding(
        &mut self,
        source_output: &str,
        target_output: &str,
        refill: tiler_core::output_selection::MigrationSourceRefill,
        sticky: &BTreeSet<WindowKey>,
    ) -> Option<MigrationOutcome> {
        if source_output == target_output {
            return None;
        }
        if !self.outputs.contains_key(source_output) || !self.outputs.contains_key(target_output) {
            return None;
        }
        // Snapshot the pre-mutation per-output previous id before any map
        // mutation (G-37 MRU eligibility input).
        let previous = self.previous.get(source_output).cloned();
        let moved = {
            let source = self.outputs.get_mut(source_output)?;
            if source.order.is_empty() || source.active >= source.order.len() {
                return None;
            }
            source.order.remove(source.active)
        };
        let moved_id = moved.id.clone();
        // Source active clamps onto the remaining scope; the refill selection
        // below activates the chosen entry explicitly. An emptied source keeps
        // active at zero with an empty order: `active_id` reads `None`, and
        // the native runtime converges ordinary trailing lifecycle before
        // verification so no unverifiable no-current is ever published.
        if let Some(source) = self.outputs.get_mut(source_output) {
            source.active = source.active.min(source.order.len().saturating_sub(1));
        }
        // Membership follows the entry wholesale for migrating (non-sticky)
        // members only: every non-sticky member's output rekeys to the target,
        // preserving workspace membership and hidden state. Sticky
        // all-desktops members stay homed on the source output and never ride
        // the moved entry. No per-window send, no admission.
        for (key, loc) in self.membership.iter_mut() {
            if loc.output == source_output && loc.workspace == moved_id && !sticky.contains(key) {
                loc.output = target_output.to_owned();
            }
        }
        // Shared G-37 selector over the remaining source scope with the
        // migrated id excluded (it is already removed above).
        let remaining: Vec<String> = self
            .outputs
            .get(source_output)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default();
        let remaining_wire: Vec<tiler_core::directional::WorkspaceId> = remaining
            .iter()
            .map(|id| tiler_core::directional::WorkspaceId(id.clone()))
            .collect();
        let previous_wire = previous.map(tiler_core::directional::WorkspaceId);
        let refill_id = tiler_core::output_selection::select_migration_source_refill(
            refill,
            previous_wire.as_ref(),
            &remaining_wire,
        )
        .map(|id| id.0.clone());
        // D4 destination: insert immediately after the target's current
        // workspace and show the moved entry.
        let insert_at = {
            let target = self.outputs.get(target_output)?;
            if target.order.is_empty() {
                0
            } else {
                (target.active + 1).min(target.order.len())
            }
        };
        if let Some(target) = self.outputs.get_mut(target_output) {
            // Sticky members never ride the moved entry: strip them before the
            // insert so the target set holds exactly the migrating members.
            // Their remembered-focus entry leaves with them (source refill
            // focus resolves from its own entry below).
            let mut moved = moved;
            for key in sticky.iter() {
                moved.members.remove(key);
                if moved.last_focus.as_ref() == Some(key) {
                    moved.last_focus = None;
                }
            }
            target.order.insert(insert_at, moved);
            target.active = insert_at;
        }
        // Preserve a still-scoped live source current view in both modes:
        // activate the selected refill when one exists.
        if let Some(refill) = refill_id.clone()
            && let Some(source) = self.outputs.get_mut(source_output)
            && let Some(pos) = source.order.iter().position(|e| e.id == refill)
        {
            source.active = pos;
        }
        // Sticky re-home: sticky members that lived on the moved id stay on
        // the source output under the selected refill, preserving hidden
        // state. With no remaining source workspace there is no refill to
        // home them to, so their locations stay untouched for lifecycle to
        // converge (never a dangling re-home, never a target claim).
        if let Some(refill) = refill_id.clone() {
            for key in sticky.iter() {
                if let Some(loc) = self.membership.get_mut(key)
                    && loc.output == source_output
                    && loc.workspace == moved_id
                {
                    loc.workspace = refill.clone();
                }
            }
            if let Some(source) = self.outputs.get_mut(source_output)
                && let Some(entry) = source.order.iter_mut().find(|e| e.id == refill)
            {
                for key in sticky.iter() {
                    if self
                        .membership
                        .get(key)
                        .is_some_and(|loc| loc.output == source_output && loc.workspace == refill)
                    {
                        entry.members.insert(key.clone());
                    }
                }
            }
        }
        // D9 history/return: out-of-source-scope previous ids clear through
        // scoped validation; hotplug-return associations drop only the
        // explicitly moved id (migrated ids lose auto return, siblings keep
        // theirs).
        for record in self.displaced.values_mut() {
            record.workspace_ids.retain(|id| *id != moved_id);
        }
        self.displaced.retain(|_, r| !r.workspace_ids.is_empty());
        self.validate_history(source_output);
        self.validate_history(target_output);
        Some(MigrationOutcome {
            moved_id,
            source_output: source_output.to_owned(),
            target_output: target_output.to_owned(),
            source_refill: refill_id,
        })
    }
}
