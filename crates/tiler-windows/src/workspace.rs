#![forbid(unsafe_code)]
//! Per-output-local managed workspaces plus digit classification.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use tiler_core::workspace as policy;

pub const VK_LWIN: u32 = 91;
pub const VK_RWIN: u32 = 92;
pub const VK_SHIFT: u32 = 16;
pub const VK_LSHIFT: u32 = 160;
pub const VK_RSHIFT: u32 = 161;
pub const VK_CTRL: u32 = 17;
pub const VK_LCTRL: u32 = 162;
pub const VK_RCTRL: u32 = 163;
pub const VK_ALT: u32 = 18;
pub const VK_LALT: u32 = 164;
pub const VK_RALT: u32 = 165;
pub const VK_0: u32 = 0x30;
pub const VK_9: u32 = 0x39;

/// Digit op shared with the unified hook classifier: unshifted selects,
/// Shift sends. Single authority lives in `snapkey::WorkspaceOp`; this alias
/// keeps the portable session-policy vocabulary stable.
pub use crate::snapkey::WorkspaceOp as DigitOp;

/// Decode one Win+digit chord. Shift selects send; Ctrl/Alt or missing Win
/// refuse. Symbol aliases share the digit VK on the same physical key.
#[must_use]
pub const fn decode_digit(
    vk: u32,
    shift: bool,
    ctrl: bool,
    alt: bool,
    win_held: bool,
) -> Option<(DigitOp, u8)> {
    if !win_held || ctrl || alt || vk < VK_0 || vk > VK_9 {
        return None;
    }
    let index = (vk - VK_0) as u8;
    let op = if shift {
        DigitOp::Send
    } else {
        DigitOp::Select
    };
    Some((op, index))
}

#[must_use]
pub const fn is_digit_vk(vk: u32) -> bool {
    vk >= VK_0 && vk <= VK_9
}

/// Stable session member identity: HWND plus process-lifetime evidence.
/// Deliberately carries no product nonce: the nonce exists only while hidden
/// and is removed on reveal, so visible membership keyed by nonce would flap.
/// Hidden claims bind the ephemeral nonce separately in the owner table and
/// the ledger; same-process HWND reuse fences on `(hwnd, creation)` plus the
/// live nonce check before any effect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowKey {
    pub hwnd: u64,
    pub pid: u32,
    pub creation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberLoc {
    pub output: String,
    pub workspace: String,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    pub id: String,
    pub token: String,
    pub members: BTreeSet<WindowKey>,
    pub last_focus: Option<WindowKey>,
}

#[derive(Debug, Clone)]
pub struct OutputState {
    pub key: String,
    pub token: String,
    pub order: Vec<WorkspaceEntry>,
    pub active: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplacedRecord {
    pub workspace_ids: Vec<String>,
    pub dest_key: String,
}

#[derive(Debug, Clone)]
pub struct ManagedWorkspaces {
    outputs: BTreeMap<String, OutputState>,
    displaced: BTreeMap<String, DisplacedRecord>,
    membership: BTreeMap<WindowKey, MemberLoc>,
    next_ws: u64,
    next_output: u64,
    /// Persisted default for newly created workspaces (KDE `defaultTiled`
    /// parity, initially true). Live default edits affect only workspaces
    /// created after the edit; existing entries keep their session state.
    /// Overrides reset on owner restart (nothing persists per workspace).
    default_tiled: bool,
    /// Session-local per-workspace tiled state, keyed by workspace id.
    /// Absent entries read as the default (see [`ManagedWorkspaces::is_tiled`]).
    tiled: BTreeMap<String, bool>,
    /// Stable previous workspace id per output (item 1 history): the actual
    /// immediately preceding observed view. Survives emptiness; clears on
    /// removal/unassignment/leaving the recording output's scope. No-op until
    /// the next recorded change; never recreated or ordinal-reinterpreted.
    previous: BTreeMap<String, String>,
    /// Last observed active workspace id per output (item 1 baseline):
    /// primed on output creation/reconnect, advanced only by
    /// [`ManagedWorkspaces::observe_workspace_change`].
    baseline: BTreeMap<String, String>,
    /// Single shared-scope history for future non-local modes (item 12/14
    /// pending): pure helpers only, no runtime observation yet.
    shared_previous: Option<String>,
    /// Single shared-scope baseline for future non-local modes: pure only.
    shared_baseline: Option<String>,
}

impl Default for ManagedWorkspaces {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of one whole-active-workspace reassignment: the moved stable id
/// plus the source refill the configured selector chose. The caller observes
/// the completed native transfer separately (item 1 observation rule);
/// this primitive only rekeys portable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    pub moved_id: String,
    pub source_output: String,
    pub target_output: String,
    /// Source refill workspace id selected after scoped invalidation, or
    /// `None` when the source keeps no workspace (ordinary trailing-empty
    /// lifecycle converges later on topology signals; nothing is recreated
    /// here).
    pub source_refill: Option<String>,
}

impl ManagedWorkspaces {
    #[must_use]
    pub fn new() -> Self {
        Self {
            outputs: BTreeMap::new(),
            displaced: BTreeMap::new(),
            membership: BTreeMap::new(),
            next_ws: 0,
            next_output: 0,
            default_tiled: true,
            tiled: BTreeMap::new(),
            previous: BTreeMap::new(),
            baseline: BTreeMap::new(),
            shared_previous: None,
            shared_baseline: None,
        }
    }

    /// Persisted default applied to newly created workspaces only.
    #[must_use]
    pub fn default_tiled(&self) -> bool {
        self.default_tiled
    }

    /// Adopt a live default change: future workspaces seed from it, existing
    /// entries keep their session state. Returns true when the value changed.
    pub fn set_default_tiled(&mut self, value: bool) -> bool {
        if self.default_tiled == value {
            return false;
        }
        self.default_tiled = value;
        true
    }

    /// Session tiled state for one workspace. Unknown workspaces and entries
    /// without an explicit override read as the current default.
    #[must_use]
    pub fn is_tiled(&self, output: &str, workspace: &str) -> bool {
        let known = self
            .outputs
            .get(output)
            .is_some_and(|s| s.order.iter().any(|e| e.id == workspace));
        if !known {
            return self.default_tiled;
        }
        self.tiled
            .get(workspace)
            .copied()
            .unwrap_or(self.default_tiled)
    }

    /// Set the session tiled state for one workspace. Returns false and
    /// changes nothing when the workspace is unknown on the output.
    pub fn set_tiled(&mut self, output: &str, workspace: &str, tiled: bool) -> bool {
        let known = self
            .outputs
            .get(output)
            .is_some_and(|s| s.order.iter().any(|e| e.id == workspace));
        if !known {
            return false;
        }
        self.tiled.insert(workspace.to_owned(), tiled);
        true
    }

    /// Tiled state of the output's active workspace, or `None` when the
    /// output is unknown. Truthful tray scope: the caller disables the
    /// checkbox on `None`, never guesses.
    #[must_use]
    pub fn current_tiled(&self, output: &str) -> Option<bool> {
        let active = self.active_id(output)?;
        Some(self.is_tiled(output, &active))
    }

    /// Seed one workspace id from the current default when it carries no
    /// explicit session state yet.
    fn seed_mode(&mut self, id: &str) {
        let default = self.default_tiled;
        self.tiled.entry(id.to_owned()).or_insert(default);
    }

    /// Drop session state for workspaces that no longer exist.
    fn prune_modes(&mut self) {
        let live: BTreeSet<String> = self
            .outputs
            .values()
            .flat_map(|s| s.order.iter().map(|e| e.id.clone()))
            .collect();
        self.tiled.retain(|id, _| live.contains(id));
    }

    pub fn ensure_output(&mut self, key: &str) -> &mut OutputState {
        if !self.outputs.contains_key(key) {
            self.next_output += 1;
            let token = format!("o{}", self.next_output);
            let mut state = OutputState {
                key: key.to_owned(),
                token,
                order: Vec::new(),
                active: 0,
            };
            for _ in 0..policy::MIN_WORKSPACES {
                self.next_ws += 1;
                let id = format!("ws-{}", self.next_ws);
                self.seed_mode(&id);
                state.order.push(WorkspaceEntry {
                    id,
                    token: format!("ws{}", self.next_ws),
                    members: BTreeSet::new(),
                    last_focus: None,
                });
            }
            self.outputs.insert(key.to_owned(), state);
            // Prime a fresh observation baseline for the new output: the
            // initial active view with no previous yet. Subsequent observed
            // displacement/return changes record from here.
            if let Some(active) = self.active_id(key) {
                self.baseline.insert(key.to_owned(), active);
            }
        }
        self.outputs.get_mut(key).expect("ensured output")
    }

    #[must_use]
    pub fn workspace_count(&self, output: &str) -> usize {
        self.outputs.get(output).map_or(0, |o| o.order.len())
    }

    #[must_use]
    pub fn active_id(&self, output: &str) -> Option<String> {
        self.outputs
            .get(output)
            .and_then(|o| o.order.get(o.active).map(|w| w.id.clone()))
    }

    /// Stable previous workspace id for one output (item 1 toggle target).
    /// `None` until a recorded change exists or after invalidation clears it.
    #[must_use]
    pub fn history_previous(&self, output: &str) -> Option<String> {
        self.previous.get(output).cloned()
    }

    /// Last observed active workspace id for one output (item 1 baseline).
    #[must_use]
    pub fn history_baseline(&self, output: &str) -> Option<String> {
        self.baseline.get(output).cloned()
    }

    /// Record one successfully observed view change on an output. Only
    /// completed transitions call this (verified hide/reveal select,
    /// verified send-follow arrival, native/foreground activation, hotplug):
    /// never attempted setters, same-workspace activation, or output focus
    /// alone. Returns true when a new previous entry was recorded.
    ///
    /// Rules: unknown outputs or unknown current ids never record; a missing
    /// baseline primes to current with no previous; an unchanged baseline is
    /// a no-op (validating a stale previous); a changed baseline stores the
    /// old baseline as previous unless it left scope (then previous clears
    /// and no valid toggle exists until the next recorded change).
    pub fn observe_workspace_change(&mut self, output: &str, current: &str) -> bool {
        let live: BTreeSet<String> = match self.outputs.get(output) {
            Some(state) => state.order.iter().map(|e| e.id.clone()).collect(),
            None => return false,
        };
        if !live.contains(current) {
            self.validate_history(output);
            return false;
        }
        match self.baseline.get(output).cloned() {
            None => {
                self.baseline.insert(output.to_owned(), current.to_owned());
                self.validate_history(output);
                false
            }
            Some(base) if base == current => {
                self.validate_history(output);
                false
            }
            Some(base) => {
                if live.contains(&base) {
                    self.previous.insert(output.to_owned(), base);
                } else {
                    self.previous.remove(output);
                }
                self.baseline.insert(output.to_owned(), current.to_owned());
                self.validate_history(output);
                self.previous.contains_key(output)
            }
        }
    }

    /// Resolve the previous-view toggle target without activating, appending,
    /// or creating. Validates the stable id (clears removed/unassigned or
    /// out-of-scope entries like removal); `None` is a no-op until the next
    /// recorded change. Returning the active id itself is a valid no-op
    /// target: the caller treats it as already-there with no native effect.
    pub fn resolve_previous(&mut self, output: &str) -> Option<String> {
        let id = self.previous.get(output).cloned()?;
        let live = self.outputs.get(output)?;
        if live.order.iter().any(|e| e.id == id) {
            Some(id)
        } else {
            self.previous.remove(output);
            None
        }
    }

    /// Resolve an ordinal ring step without activating, appending, or
    /// creating. The ring is every existing scoped id in order, including the
    /// trailing empty and ordinals beyond 9; first/last wrap. `delta` must be
    /// -1 (previous) or +1 (next); anything else refuses.
    pub fn resolve_relative(&self, output: &str, delta: i32) -> Option<String> {
        let current = self
            .outputs
            .get(output)?
            .order
            .get(self.outputs.get(output)?.active)?
            .id
            .clone();
        self.resolve_relative_from(output, &current, delta)
    }

    /// Resolve an ordinal ring step from an explicit current workspace id
    /// (item 2 sends resolve from the mover's source workspace, which is
    /// usually but not always the active view). Pure: no activation, append,
    /// or creation. Filling the resolved trailing empty invokes ordinary
    /// lifecycle for the next spare after transfer; the frozen target never
    /// re-resolves. `delta` must be -1 or +1; unknown output/current or an
    /// empty ring refuses.
    pub fn resolve_relative_from(&self, output: &str, current: &str, delta: i32) -> Option<String> {
        if delta != -1 && delta != 1 {
            return None;
        }
        let state = self.outputs.get(output)?;
        if state.order.is_empty() {
            return None;
        }
        let ring: Vec<String> = state.order.iter().map(|e| e.id.clone()).collect();
        let at = ring.iter().position(|id| id == current)?;
        let next = ring[(at as i32 + delta + ring.len() as i32) as usize % ring.len()].clone();
        Some(next)
    }

    /// Scoped ring ids for one output: every existing id in order, including
    /// the trailing empty and ordinals beyond 9. Selection creates nothing.
    #[must_use]
    pub fn scoped_ring_ids(&self, output: &str) -> Vec<String> {
        self.workspace_ids(output)
    }

    /// Drop both the previous entry and the observed baseline for one
    /// output (disconnect handling). Displacement associations stay separate.
    pub fn discard_output_history(&mut self, output: &str) {
        self.previous.remove(output);
        self.baseline.remove(output);
    }

    /// Validate one output's previous entry against live scope: a removed,
    /// unassigned, or out-of-scope id clears. Never reinterprets an ordinal
    /// or recreates a workspace.
    fn validate_history(&mut self, output: &str) {
        let live: Option<BTreeSet<String>> = self
            .outputs
            .get(output)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect());
        let Some(live) = live else {
            self.previous.remove(output);
            self.baseline.remove(output);
            return;
        };
        if let Some(prev) = self.previous.get(output).cloned()
            && !live.contains(&prev)
        {
            self.previous.remove(output);
        }
        if let Some(base) = self.baseline.get(output).cloned()
            && !live.contains(&base)
        {
            // Re-prime a removed baseline to the live active view so the next
            // observed change records honestly instead of dangling.
            if let Some(active) = self.active_id(output) {
                self.baseline.insert(output.to_owned(), active);
            } else {
                self.baseline.remove(output);
            }
        }
    }

    /// Pure shared-scope observation for future non-local modes (pending
    /// runtime): same record/no-op/invalidate rules over one history/ring.
    /// `live` is the full shared order; `current` is the observed view.
    pub fn observe_shared_change(&mut self, live: &[String], current: &str) -> bool {
        if !live.contains(&current.to_owned()) {
            self.validate_shared(live);
            return false;
        }
        match self.shared_baseline.clone() {
            None => {
                self.shared_baseline = Some(current.to_owned());
                self.validate_shared(live);
                false
            }
            Some(base) if base == current => {
                self.validate_shared(live);
                false
            }
            Some(base) => {
                if live.contains(&base) {
                    self.shared_previous = Some(base);
                } else {
                    self.shared_previous = None;
                }
                self.shared_baseline = Some(current.to_owned());
                self.validate_shared(live);
                self.shared_previous.is_some()
            }
        }
    }

    /// Pure shared-scope previous resolver (pending runtime): validates the
    /// stable id against `live`, clearing removed entries.
    pub fn resolve_shared_previous(&mut self, live: &[String]) -> Option<String> {
        let id = self.shared_previous.clone()?;
        if live.contains(&id) {
            Some(id)
        } else {
            self.shared_previous = None;
            None
        }
    }

    /// Pure shared-scope ring step (pending runtime): wraps over the full
    /// shared order including trailing empty and beyond-9 positions.
    #[must_use]
    pub fn resolve_shared_relative(live: &[String], current: &str, delta: i32) -> Option<String> {
        if delta != -1 && delta != 1 || live.is_empty() {
            return None;
        }
        let at = live.iter().position(|id| id == current)?;
        Some(live[(at as i32 + delta + live.len() as i32) as usize % live.len()].clone())
    }

    fn validate_shared(&mut self, live: &[String]) {
        if let Some(prev) = self.shared_previous.clone()
            && !live.contains(&prev)
        {
            self.shared_previous = None;
        }
        if let Some(base) = self.shared_baseline.clone()
            && !live.contains(&base)
        {
            self.shared_baseline = None;
        }
    }

    /// Activate one workspace by id without disturbing order. Returns false
    /// when the output or workspace is unknown.
    pub fn activate(&mut self, output: &str, workspace: &str) -> bool {
        let Some(state) = self.outputs.get_mut(output) else {
            return false;
        };
        let Some(pos) = state.order.iter().position(|e| e.id == workspace) else {
            return false;
        };
        state.active = pos;
        true
    }

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

    /// Append an empty entry carrying an existing global id (shared-mode
    /// convergence, item 12 stage B): the logical shared id joins this
    /// output-bound domain so every live output can show the same id while
    /// keeping its own Engine layout domain. The entry mints a fresh unique
    /// token, seeds the default tiled mode only when the id carries no
    /// session state yet (an id live elsewhere keeps its flag), and never
    /// activates: the caller activates after the verified transition.
    /// Trailing lifecycle still owns creation of brand-new ids; this only
    /// converges an already-live shared id. Returns true when created.
    pub fn ensure_workspace_id(&mut self, output: &str, id: &str) -> bool {
        let known = self
            .outputs
            .get(output)
            .is_some_and(|s| s.order.iter().any(|e| e.id == id));
        if known || !self.outputs.contains_key(output) {
            return false;
        }
        self.next_ws += 1;
        let token = format!("ws{}", self.next_ws);
        self.seed_mode(id);
        let Some(state) = self.outputs.get_mut(output) else {
            return false;
        };
        state.order.push(WorkspaceEntry {
            id: id.to_owned(),
            token,
            members: BTreeSet::new(),
            last_focus: None,
        });
        true
    }

    /// Mint one brand-new logical shared id and append an empty entry for it
    /// on every listed output (item 12 stage B shared trailing creation).
    /// The id is fresh (`ws-{next}`); each output mints its own fresh unique
    /// token for the same id (per-output Engine layout domains stay
    /// distinct), seeds the default tiled mode once, and never activates:
    /// the caller selects after. Outputs missing from the map are skipped;
    /// outputs already carrying the id (impossible for a fresh id) keep
    /// theirs. Returns the new id, or an empty string when no output was
    /// listed (nothing minted, nothing appended).
    pub fn mint_shared_id(&mut self, outputs: &[String]) -> String {
        if outputs.is_empty() {
            return String::new();
        }
        self.next_ws += 1;
        let id = format!("ws-{}", self.next_ws);
        self.seed_mode(&id);
        for output in outputs {
            if self
                .outputs
                .get(output)
                .is_none_or(|s| s.order.iter().all(|e| e.id != id))
            {
                self.next_ws += 1;
                let token = format!("ws{}", self.next_ws);
                if let Some(state) = self.outputs.get_mut(output) {
                    state.order.push(WorkspaceEntry {
                        id: id.clone(),
                        token,
                        members: BTreeSet::new(),
                        last_focus: None,
                    });
                }
            }
        }
        id
    }

    /// Strip logical shared ids from every output-bound domain at once (item
    /// 12 stage B shared trailing cleanup). Entries name removed ids; member
    /// sets of removed entries are already empty of non-sticky members by
    /// construction (only invisible empty non-final ids plan removal), while
    /// sticky float state stays owned by the owner tables exactly like the
    /// single-output path. Per-output actives clamp onto the surviving
    /// scope, session modes/displaced records prune to live, and each
    /// output's local previous/baseline validates like
    /// [`ManagedWorkspaces::apply_cleanup`]. Scoped (non-local) history
    /// validation stays with the scope owner; nothing recreates here.
    pub fn remove_shared_ids(&mut self, removed: &[String]) {
        if removed.is_empty() {
            return;
        }
        for state in self.outputs.values_mut() {
            state.order.retain(|e| !removed.contains(&e.id));
        }
        for state in self.outputs.values_mut() {
            if state.order.is_empty() {
                state.active = 0;
            } else {
                state.active = state.active.min(state.order.len() - 1);
            }
        }
        self.prune_modes();
        let live: BTreeSet<String> = self
            .outputs
            .values()
            .flat_map(|s| s.order.iter().map(|e| e.id.clone()))
            .collect();
        for record in self.displaced.values_mut() {
            record.workspace_ids.retain(|id| live.contains(id));
        }
        self.displaced.retain(|_, r| !r.workspace_ids.is_empty());
        let outputs: Vec<String> = self.outputs.keys().cloned().collect();
        for output in &outputs {
            self.validate_history(output);
        }
    }

    pub fn select(&mut self, output: &str, index: u8) -> Option<String> {
        let state = self.outputs.get_mut(output)?;
        let pos = policy::select_existing(state.order.len(), index)?;
        state.active = pos;
        Some(state.order[pos].id.clone())
    }

    pub fn select_trailing(&mut self, output: &str) -> Option<(String, bool)> {
        let reuse = self.outputs.get_mut(output).and_then(|state| {
            if state.order.is_empty() {
                return None;
            }
            let last = state.order.len() - 1;
            if state.order[last].members.is_empty() {
                state.active = last;
                Some(state.order[last].id.clone())
            } else {
                None
            }
        });
        if let Some(id) = reuse {
            return Some((id, false));
        }
        if self.outputs.get(output).is_none_or(|s| s.order.is_empty()) {
            return None;
        }
        self.next_ws += 1;
        let id = format!("ws-{}", self.next_ws);
        let token = format!("ws{}", self.next_ws);
        self.seed_mode(&id);
        let state = self.outputs.get_mut(output)?;
        state.order.push(WorkspaceEntry {
            id: id.clone(),
            token,
            members: BTreeSet::new(),
            last_focus: None,
        });
        state.active = state.order.len() - 1;
        Some((id, true))
    }

    #[must_use]
    pub fn resolve_send(&self, output: &str, index: u8) -> Option<String> {
        let state = self.outputs.get(output)?;
        let pos = policy::resolve_send_target(state.order.len(), index)?;
        Some(state.order[pos].id.clone())
    }

    pub fn resolve_send_trailing(&mut self, output: &str) -> Option<(String, bool)> {
        let reuse = self.outputs.get(output).and_then(|state| {
            if state.order.is_empty() {
                return None;
            }
            let last = state.order.len() - 1;
            if state.order[last].members.is_empty() {
                Some(state.order[last].id.clone())
            } else {
                None
            }
        });
        if let Some(id) = reuse {
            return Some((id, false));
        }
        if self.outputs.get(output).is_none_or(|s| s.order.is_empty()) {
            return None;
        }
        self.next_ws += 1;
        let id = format!("ws-{}", self.next_ws);
        let token = format!("ws{}", self.next_ws);
        self.seed_mode(&id);
        let state = self.outputs.get_mut(output)?;
        state.order.push(WorkspaceEntry {
            id: id.clone(),
            token,
            members: BTreeSet::new(),
            last_focus: None,
        });
        Some((id, true))
    }

    /// Assign a window; returns false and changes nothing when the workspace
    /// does not exist on the output.
    pub fn assign(
        &mut self,
        window: WindowKey,
        output: &str,
        workspace: &str,
        hidden: bool,
    ) -> bool {
        let known = self
            .outputs
            .get(output)
            .is_some_and(|s| s.order.iter().any(|e| e.id == workspace));
        if !known {
            return false;
        }
        for state in self.outputs.values_mut() {
            for entry in &mut state.order {
                entry.members.remove(&window);
            }
        }
        if let Some(state) = self.outputs.get_mut(output)
            && let Some(entry) = state.order.iter_mut().find(|e| e.id == workspace)
        {
            entry.members.insert(window.clone());
        }
        self.membership.insert(
            window,
            MemberLoc {
                output: output.to_owned(),
                workspace: workspace.to_owned(),
                hidden,
            },
        );
        true
    }

    pub fn set_hidden(&mut self, window: &WindowKey, hidden: bool) {
        if let Some(loc) = self.membership.get_mut(window) {
            loc.hidden = hidden;
        }
    }

    #[must_use]
    pub fn member_loc(&self, window: &WindowKey) -> Option<&MemberLoc> {
        self.membership.get(window)
    }

    #[must_use]
    pub fn is_hidden(&self, window: &WindowKey) -> bool {
        self.membership.get(window).is_some_and(|l| l.hidden)
    }

    pub fn note_foreground(&mut self, window: &WindowKey) {
        let Some(loc) = self.membership.get(window).cloned() else {
            return;
        };
        if let Some(state) = self.outputs.get_mut(&loc.output)
            && let Some(entry) = state.order.iter_mut().find(|e| e.id == loc.workspace)
        {
            entry.last_focus = Some(window.clone());
        }
    }

    /// Remembered focus for one workspace: the last focused member still
    /// holding membership, or `None` when nothing is remembered. Unlike
    /// [`ManagedWorkspaces::focus_target`] this never substitutes another
    /// member: migration follow uses exactly the remembered client (or no
    /// focus when it stayed sticky, left, or never existed).
    #[must_use]
    pub fn remembered_focus(&self, output: &str, workspace: &str) -> Option<WindowKey> {
        let state = self.outputs.get(output)?;
        let entry = state.order.iter().find(|e| e.id == workspace)?;
        entry
            .last_focus
            .clone()
            .filter(|last| entry.members.contains(last))
    }

    #[must_use]
    pub fn focus_target(
        &self,
        output: &str,
        workspace: &str,
        visible: &BTreeSet<WindowKey>,
    ) -> Option<WindowKey> {
        let state = self.outputs.get(output)?;
        let entry = state.order.iter().find(|e| e.id == workspace)?;
        if let Some(last) = &entry.last_focus
            && entry.members.contains(last)
            && visible.contains(last)
        {
            return Some(last.clone());
        }
        entry.members.iter().find(|w| visible.contains(*w)).cloned()
    }

    /// Post-reveal eligible visible set for focus: non-hidden members whose
    /// Engine token is present in the fresh focus observation. The caller
    /// unions eligible-observed tokens with verified retained maximized
    /// tokens (focus-only; geometry still excludes them), so a maximized
    /// mover stays focusable. Pure so the fresh-focus regression pins it
    /// without native calls: pre-switch membership alone is never fresh
    /// enough, and hidden/frameless members without a fresh token never
    /// take focus.
    #[must_use]
    pub fn eligible_focus_set(
        &self,
        members: &BTreeSet<WindowKey>,
        member_tokens: &BTreeMap<WindowKey, String>,
        fresh_tokens: &HashSet<String>,
    ) -> BTreeSet<WindowKey> {
        members
            .iter()
            .filter(|k| !self.is_hidden(k))
            .filter(|k| {
                member_tokens
                    .get(*k)
                    .is_some_and(|t| fresh_tokens.contains(t))
            })
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn select_for_foreground(&self, window: &WindowKey) -> Option<(String, String)> {
        let loc = self.membership.get(window)?;
        if !loc.hidden {
            return None;
        }
        Some((loc.output.clone(), loc.workspace.clone()))
    }

    pub fn plan_cleanup(
        &self,
        output: &str,
        visible_ids: &[String],
        retained_ids: &[String],
    ) -> (Vec<String>, bool) {
        self.plan_cleanup_excluding(output, visible_ids, retained_ids, &BTreeSet::new())
    }

    /// Trailing-empty plan that never lets sticky members occupy their backing
    /// workspace (KDE `occupiedIds` global-sticky exclusion parity). Sticky
    /// members stay members but count as empty for occupancy, so a
    /// sticky-only workspace prunes like an empty one while the sticky float
    /// state itself is retained by the owner tables. Pure over the same
    /// policy as [`ManagedWorkspaces::plan_cleanup`].
    pub fn plan_cleanup_excluding(
        &self,
        output: &str,
        visible_ids: &[String],
        retained_ids: &[String],
        sticky: &BTreeSet<WindowKey>,
    ) -> (Vec<String>, bool) {
        let Some(state) = self.outputs.get(output) else {
            return (Vec::new(), false);
        };
        let facts: Vec<policy::WorkspaceFacts> = state
            .order
            .iter()
            .map(|e| {
                let non_sticky_empty = e.members.iter().all(|m| sticky.contains(m));
                let occupied = (!e.members.is_empty() && !non_sticky_empty)
                    || self.membership.iter().any(|(k, l)| {
                        l.output == output && l.workspace == e.id && !sticky.contains(k)
                    });
                policy::WorkspaceFacts {
                    empty: non_sticky_empty && !occupied,
                    visible: visible_ids.iter().any(|v| v == &e.id),
                    occupied,
                    retained: retained_ids.iter().any(|r| r == &e.id)
                        || self
                            .displaced
                            .values()
                            .any(|d| d.workspace_ids.contains(&e.id)),
                }
            })
            .collect();
        let plan = policy::plan_trailing(&facts);
        let removed: Vec<String> = plan
            .remove
            .iter()
            .map(|&i| state.order[i].id.clone())
            .collect();
        (removed, plan.need_append)
    }

    pub fn apply_cleanup(&mut self, output: &str, removed: &[String], append: bool) {
        let active_id = self.active_id(output);
        let mut seeded: Vec<String> = Vec::new();
        if let Some(state) = self.outputs.get_mut(output) {
            state.order.retain(|e| !removed.contains(&e.id));
            if append {
                self.next_ws += 1;
                let id = format!("ws-{}", self.next_ws);
                let token = format!("ws{}", self.next_ws);
                seeded.push(id.clone());
                state.order.push(WorkspaceEntry {
                    id,
                    token,
                    members: BTreeSet::new(),
                    last_focus: None,
                });
            }
            while state.order.len() < policy::MIN_WORKSPACES {
                self.next_ws += 1;
                let id = format!("ws-{}", self.next_ws);
                let token = format!("ws{}", self.next_ws);
                seeded.push(id.clone());
                state.order.push(WorkspaceEntry {
                    id,
                    token,
                    members: BTreeSet::new(),
                    last_focus: None,
                });
            }
            state.active = active_id
                .and_then(|id| state.order.iter().position(|e| e.id == id))
                .unwrap_or(0)
                .min(state.order.len().saturating_sub(1));
        }
        for id in &seeded {
            self.seed_mode(id);
        }
        self.prune_modes();
        let live: BTreeSet<String> = self
            .outputs
            .values()
            .flat_map(|s| s.order.iter().map(|e| e.id.clone()))
            .collect();
        for record in self.displaced.values_mut() {
            record.workspace_ids.retain(|id| live.contains(id));
        }
        self.displaced.retain(|_, r| !r.workspace_ids.is_empty());
        // History invalidation: a removed/unassigned previous clears; a
        // removed baseline re-primes to the live active view. Surviving empty
        // workspaces stay valid; ordinals are never reinterpreted and nothing
        // is recreated here.
        self.validate_history(output);
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

    #[must_use]
    pub fn workspace_members(&self, output: &str, workspace: &str) -> BTreeSet<WindowKey> {
        self.outputs
            .get(output)
            .and_then(|s| s.order.iter().find(|e| e.id == workspace))
            .map(|e| e.members.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn output_keys(&self) -> Vec<String> {
        self.outputs.keys().cloned().collect()
    }

    #[must_use]
    pub fn workspace_ids(&self, output: &str) -> Vec<String> {
        self.outputs
            .get(output)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn active_index(&self, output: &str) -> Option<usize> {
        self.outputs.get(output).map(|s| s.active)
    }

    /// Forget one window everywhere (close cleanup). Hidden retention is
    /// owned by the caller ledger table; this only drops session membership.
    /// Returns true when the window was known.
    pub fn remove_window(&mut self, window: &WindowKey) -> bool {
        let mut known = false;
        for state in self.outputs.values_mut() {
            for entry in &mut state.order {
                if entry.members.remove(window) {
                    known = true;
                }
                if entry.last_focus.as_ref() == Some(window) {
                    entry.last_focus = None;
                }
            }
        }
        if self.membership.remove(window).is_some() {
            known = true;
        }
        known
    }

    #[must_use]
    pub fn displaced_snapshot(&self) -> BTreeMap<String, DisplacedRecord> {
        self.displaced.clone()
    }

    #[must_use]
    pub fn output_token(&self, output: &str) -> String {
        self.outputs
            .get(output)
            .map(|s| s.token.clone())
            .unwrap_or_else(|| "o?".to_owned())
    }

    #[must_use]
    pub fn workspace_token(&self, output: &str, workspace: &str) -> String {
        self.outputs
            .get(output)
            .and_then(|s| s.order.iter().find(|e| e.id == workspace))
            .map(|e| e.token.clone())
            .unwrap_or_else(|| "ws?".to_owned())
    }
}

#[must_use]
pub fn verify_send_follow(
    mover: &WindowKey,
    source_members: &BTreeSet<WindowKey>,
    target_members: &BTreeSet<WindowKey>,
) -> bool {
    !source_members.contains(mover) && target_members.contains(mover)
}

/// Unconfirmed workspace-domain releases (workspace-mode toggle edges).
/// Keyed by `(output, workspace)`: the latest intent wins, so a failed float
/// release followed by a successful retile leaves no stale float entry that
/// a later retry could fire at the retiled layout. A confirmed toggle clears
/// its domain entry. A pending entry is stale once its workspace reads tiled
/// again: both intents imply a floating workspace, so a tiled workspace must
/// never release.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingReleases {
    inner: BTreeMap<(String, String), bool>,
}

impl PendingReleases {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    /// Queue the latest intent for one domain, overwriting any earlier one.
    pub fn queue(&mut self, output: &str, workspace: &str, retile: bool) {
        self.inner
            .insert((output.to_owned(), workspace.to_owned()), retile);
    }

    /// Drop the entry for one domain after its release confirms (or after a
    /// successful toggle that supersedes it). Returns true when one existed.
    pub fn confirm(&mut self, output: &str, workspace: &str) -> bool {
        self.inner
            .remove(&(output.to_owned(), workspace.to_owned()))
            .is_some()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Snapshot of pending `(output, workspace, retile)` intents.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(String, String, bool)> {
        self.inner
            .iter()
            .map(|((o, w), r)| (o.clone(), w.clone(), *r))
            .collect()
    }

    /// True when the intent must not fire: the workspace is known and reads
    /// tiled, so releasing would destroy the live tiled layout. Unknown
    /// workspaces are not stale (an empty-rows release still cleans up).
    #[must_use]
    pub const fn is_stale(tiled: Option<bool>) -> bool {
        matches!(tiled, Some(true))
    }
}

/// Topology fingerprint for pending-release retries: monitor devices plus
/// work/full rects, workspace order per output, member identities, and the
/// raw HWND inventory size. Retries fire only when this changes, so a pending
/// release never polls or logs on quiet ticks.
///
/// Rectangles ride as packed `(x, y, w, h)` tuples so the inputs stay
/// `Ord` for the canonical sort.
pub type TopologyArea = (String, (i32, i32, i32, i32), (i32, i32, i32, i32));

#[must_use]
pub fn topology_fingerprint(
    areas: &[TopologyArea],
    workspaces: &[(String, Vec<String>)],
    members: &[WindowKey],
    hwnd_count: usize,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    let mut sorted_areas = areas.to_vec();
    sorted_areas.sort();
    sorted_areas.hash(&mut hasher);
    let mut sorted_ws = workspaces.to_vec();
    sorted_ws.sort();
    sorted_ws.hash(&mut hasher);
    let mut sorted_members = members.to_vec();
    sorted_members.sort();
    sorted_members.hash(&mut hasher);
    hwnd_count.hash(&mut hasher);
    hasher.finish()
}

#[must_use]
pub fn workspace_event(
    output_token: &str,
    workspace_token: &str,
    members: usize,
    action: &str,
    outcome: &str,
) -> serde_json::Value {
    serde_json::json!({
        "event": "workspace",
        "output": output_token,
        "workspace": workspace_token,
        "members": members,
        "action": action,
        "outcome": outcome,
    })
}

#[cfg(test)]
#[path = "workspace/tests.rs"]
mod tests;
