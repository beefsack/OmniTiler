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
mod tests {
    use super::*;

    fn key(hwnd: u64) -> WindowKey {
        WindowKey {
            hwnd,
            pid: 1000 + hwnd as u32,
            creation: format!("c{hwnd:016x}"),
        }
    }

    fn order_ids(m: &ManagedWorkspaces, output: &str) -> Vec<String> {
        m.outputs
            .get(output)
            .map(|s| s.order.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn decode_digit_alias_and_modifiers() {
        assert_eq!(
            decode_digit(VK_0 + 1, false, false, false, true),
            Some((DigitOp::Select, 1))
        );
        assert_eq!(
            decode_digit(VK_0 + 1, true, false, false, true),
            Some((DigitOp::Send, 1))
        );
        assert_eq!(decode_digit(VK_0 + 1, false, true, false, true), None);
        assert_eq!(decode_digit(VK_0 + 1, false, false, true, true), None);
        assert_eq!(decode_digit(VK_0 + 1, false, false, false, false), None);
        assert_eq!(decode_digit(0x41, false, false, false, true), None);
        assert!(is_digit_vk(VK_0) && is_digit_vk(VK_9));
        assert!(!is_digit_vk(0x41));
    }

    #[test]
    fn select_existing_never_creates() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        assert_eq!(m.workspace_count("mon-1"), 2);
        let first = m.select("mon-1", 1).expect("select 1");
        assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
        assert!(m.select("mon-1", 9).is_none());
        assert_eq!(m.workspace_count("mon-1"), 2);
    }

    #[test]
    fn trailing_zero_reuses_then_creates() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let (id, created) = m.select_trailing("mon-1").expect("trailing");
        assert!(!created);
        assert_eq!(m.active_id("mon-1").as_deref(), Some(id.as_str()));
        let ids = order_ids(&m, "mon-1");
        for (i, ws) in ids.iter().enumerate() {
            assert!(m.assign(key(100 + i as u64), "mon-1", ws, false));
        }
        let (_, created) = m.select_trailing("mon-1").expect("append");
        assert!(created);
        assert_eq!(m.workspace_count("mon-1"), 3);
    }

    #[test]
    fn invalid_assign_refuses_without_loss() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        let w = key(11);
        assert!(m.assign(w.clone(), "mon-1", &ws1, false));
        assert!(!m.assign(key(12), "mon-1", "ws-missing", false));
        assert!(!m.assign(key(13), "mon-missing", "ws-1", false));
        assert_eq!(m.member_loc(&w).expect("loc").workspace, ws1);
        let entry = m
            .outputs
            .get("mon-1")
            .expect("o")
            .order
            .iter()
            .find(|e| e.id == ws1)
            .expect("e");
        assert!(entry.members.contains(&w));
    }

    #[test]
    fn cleanup_preserves_active_id_across_earlier_prune() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        for i in 0..2 {
            let ids = order_ids(&m, "mon-1");
            for ws in &ids {
                let occupied = m
                    .outputs
                    .get("mon-1")
                    .expect("o")
                    .order
                    .iter()
                    .find(|e| &e.id == ws)
                    .expect("e")
                    .members
                    .is_empty();
                if occupied {
                    assert!(m.assign(key(500 + i), "mon-1", ws, false));
                    break;
                }
            }
            let _ = m.select_trailing("mon-1");
        }
        let active = m.active_id("mon-1").expect("active");
        let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
        assert!(!removed.contains(&active));
        m.apply_cleanup("mon-1", &removed, append);
        assert_eq!(m.active_id("mon-1").as_deref(), Some(active.as_str()));
        assert!(m.workspace_count("mon-1") >= policy::MIN_WORKSPACES);
    }

    #[test]
    fn activate_by_id_covers_trailing_past_ordinals() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        // Fill every workspace so trailing appends past the Win1..9 range.
        for i in 0..10 {
            let ids = order_ids(&m, "mon-1");
            for ws in &ids {
                let occupied = m
                    .outputs
                    .get("mon-1")
                    .expect("o")
                    .order
                    .iter()
                    .find(|e| &e.id == ws)
                    .expect("e")
                    .members
                    .is_empty();
                if occupied {
                    assert!(m.assign(key(900 + i), "mon-1", ws, false));
                    break;
                }
            }
            let _ = m.select_trailing("mon-1");
        }
        assert!(m.workspace_count("mon-1") > 9);
        let last = order_ids(&m, "mon-1").last().cloned().expect("last");
        let first = order_ids(&m, "mon-1").first().cloned().expect("first");
        assert!(m.activate("mon-1", &last));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(last.as_str()));
        assert!(m.activate("mon-1", &first));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
        assert!(!m.activate("mon-1", "ws-missing"));
        assert!(!m.activate("mon-missing", &first));
    }

    #[test]
    fn per_output_isolation() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        m.select("mon-1", 2);
        assert_eq!(m.outputs.get("mon-1").expect("a").active, 1);
        assert_eq!(m.outputs.get("mon-1").expect("a").active, 1);
        let a = m.resolve_send("mon-1", 1).expect("send target");
        let b = m.resolve_send("mon-2", 1).expect("send target");
        assert_ne!(a, b);
    }

    #[test]
    fn hidden_members_retained_and_foreground_selects_hidden() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let target = m.resolve_send("mon-1", 2).expect("ws2");
        let w = key(42);
        assert!(m.assign(w.clone(), "mon-1", &target, false));
        m.set_hidden(&w, true);
        assert!(m.is_hidden(&w));
        assert_eq!(m.member_loc(&w).expect("loc").workspace, target);
        let selected = m.select_for_foreground(&w).expect("foreground select");
        assert_eq!(selected, ("mon-1".to_owned(), target.clone()));
        let v = key(43);
        assert!(m.assign(v.clone(), "mon-1", &target, false));
        assert_eq!(m.select_for_foreground(&v), None);
    }

    #[test]
    fn focus_target_avoids_hidden_previous() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        let a = key(1);
        let b = key(2);
        assert!(m.assign(a.clone(), "mon-1", &ws2, false));
        assert!(m.assign(b.clone(), "mon-1", &ws2, false));
        m.note_foreground(&a);
        m.set_hidden(&a, true);
        let visible: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
        assert_eq!(m.focus_target("mon-1", &ws2, &visible), Some(b));
        assert_eq!(m.focus_target("mon-1", &ws1, &BTreeSet::new()), None);
    }

    #[test]
    fn send_follow_refuses_stale_and_wrong_target() {
        let mover = key(7);
        let other = key(8);
        let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
        let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
        assert!(verify_send_follow(&mover, &source, &target));
        let source: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
        assert!(!verify_send_follow(&mover, &source, &target));
        let target: BTreeSet<WindowKey> = [other].into_iter().collect();
        assert!(!verify_send_follow(&mover, &BTreeSet::new(), &target));
    }

    #[test]
    fn cleanup_floor_keeps_minimum_two() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let active = m.active_id("mon-1").expect("active");
        // Both workspaces empty and invisible: the floor keeps both.
        let (removed, append) = m.plan_cleanup("mon-1", &[], &[]);
        m.apply_cleanup("mon-1", &removed, append);
        assert!(m.workspace_count("mon-1") >= policy::MIN_WORKSPACES);
        assert!(m.active_id("mon-1").is_some());
        let _ = active;
    }

    #[test]
    fn hidden_members_retain_workspace_through_cleanup() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        let w = key(77);
        assert!(m.assign(w.clone(), "mon-1", &ws2, false));
        m.set_hidden(&w, true);
        let active = m.active_id("mon-1").expect("active");
        // The hidden-occupied workspace is protected from pruning.
        let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
        assert!(!removed.contains(&ws2));
        m.apply_cleanup("mon-1", &removed, append);
        assert_eq!(m.member_loc(&w).expect("loc").workspace, ws2);
        assert!(m.is_hidden(&w));
    }

    #[test]
    fn last_focus_is_per_workspace() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        let a = key(101);
        let b = key(102);
        assert!(m.assign(a.clone(), "mon-1", &ws1, false));
        assert!(m.assign(b.clone(), "mon-1", &ws2, false));
        m.note_foreground(&a);
        m.note_foreground(&b);
        let visible_a: BTreeSet<WindowKey> = [a.clone()].into_iter().collect();
        let visible_b: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
        assert_eq!(m.focus_target("mon-1", &ws1, &visible_a), Some(a));
        assert_eq!(m.focus_target("mon-1", &ws2, &visible_b), Some(b));
    }

    #[test]
    fn remove_window_cleans_membership_and_focus() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        let w = key(21);
        assert!(m.assign(w.clone(), "mon-1", &ws1, false));
        m.note_foreground(&w);
        assert!(m.remove_window(&w));
        assert!(m.member_loc(&w).is_none());
        assert!(!m.remove_window(&w));
    }

    #[test]
    fn displaced_units_never_merge_and_return_without_baseline() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let before = m.workspace_count("mon-2");
        let returning_len = m.workspace_count("mon-1");
        let w = key(99);
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        assert!(m.assign(w, "mon-1", &ws2, false));
        assert!(m.displace_output_to("mon-1", "mon-2"));
        assert!(!m.outputs.contains_key("mon-1"));
        assert!(!m.displace_output_to("mon-1", "mon-2"));
        assert!(!m.displace_output_to("mon-2", "mon-2"));
        assert!(m.reconnect_output("mon-1"));
        assert_eq!(
            m.workspace_count("mon-1"),
            returning_len,
            "no baseline duplicates"
        );
        assert_eq!(m.workspace_count("mon-2"), before);
        assert!(m.active_id("mon-1").is_some());
        assert!(!m.reconnect_output("mon-1-replaced"));
    }

    #[test]
    fn workspace_event_is_opaque() {
        let ev = workspace_event("o1", "ws2", 3, "select", "ok");
        assert_eq!(ev["output"], serde_json::Value::from("o1"));
        assert_eq!(ev["members"], serde_json::Value::from(3));
        let text = ev.to_string();
        assert!(!text.contains("hwnd") && !text.contains("pid"));
    }

    #[test]
    fn select_resolution_does_not_preactivate() {
        // Regression for `workspace_tick` mutating ACTIVE via
        // `select`/`select_trailing` before `workspace_do_select` reads
        // current (which then reports `already-active` with no native
        // effects). Select targets must resolve without activating; only the
        // transition itself activates after hide/reveal.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let before = m.active_id("mon-1").expect("active");
        let target = m.resolve_send("mon-1", 2).expect("resolve");
        assert_ne!(before, target);
        assert_eq!(
            m.active_id("mon-1").as_deref(),
            Some(before.as_str()),
            "resolve_send must not preactivate"
        );
        let before = m.active_id("mon-1").expect("active");
        let (trailing, _) = m.resolve_send_trailing("mon-1").expect("trailing");
        assert_eq!(
            m.active_id("mon-1").as_deref(),
            Some(before.as_str()),
            "resolve_send_trailing must not preactivate"
        );
        assert!(m.activate("mon-1", &trailing));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(trailing.as_str()));
    }

    #[test]
    fn eligible_focus_uses_only_fresh_observation() {
        // Post-reveal focus must use the fresh eligible observation, never the
        // pre-switch membership alone: hidden and frameless members without a
        // fresh token never take focus, and the last appropriate fresh member
        // wins.
        use std::collections::{BTreeMap, HashSet};
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        let a = key(1);
        let b = key(2);
        assert!(m.assign(a.clone(), "mon-1", &ws2, false));
        assert!(m.assign(b.clone(), "mon-1", &ws2, false));
        m.note_foreground(&a);
        m.set_hidden(&a, true);
        let mut member_tokens: BTreeMap<WindowKey, String> = BTreeMap::new();
        member_tokens.insert(a.clone(), "tok-a".to_owned());
        member_tokens.insert(b.clone(), "tok-b".to_owned());
        // Only `b` is freshly observed eligible; `a` stays hidden.
        let fresh: HashSet<String> = ["tok-b".to_owned()].into_iter().collect();
        let members: BTreeSet<WindowKey> = [a.clone(), b.clone()].into_iter().collect();
        let eligible = m.eligible_focus_set(&members, &member_tokens, &fresh);
        assert_eq!(eligible, [b.clone()].into_iter().collect());
        assert_eq!(m.focus_target("mon-1", &ws2, &eligible), Some(b));
        // Empty fresh observation means an empty target: no focus.
        let empty = m.eligible_focus_set(&members, &member_tokens, &HashSet::new());
        assert!(empty.is_empty());
        assert_eq!(m.focus_target("mon-1", &ws2, &empty), None);
    }

    #[test]
    fn eligible_focus_includes_retained_maximized_token() {
        // A maximized mover is retained, never eligible-observed: the caller
        // unions its verified retained token into the fresh focus set (focus
        // only; geometry still excludes it), so the follow/return focus
        // target resolves to the maximized member with exact foreground proof.
        use std::collections::{BTreeMap, HashSet};
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        let maxed = key(1);
        let sibling = key(2);
        assert!(m.assign(maxed.clone(), "mon-1", &ws2, false));
        assert!(m.assign(sibling.clone(), "mon-1", &ws2, false));
        m.note_foreground(&maxed);
        let mut member_tokens: BTreeMap<WindowKey, String> = BTreeMap::new();
        member_tokens.insert(maxed.clone(), "tok-max".to_owned());
        member_tokens.insert(sibling.clone(), "tok-sib".to_owned());
        let members: BTreeSet<WindowKey> = [maxed.clone(), sibling.clone()].into_iter().collect();
        // Eligible-only set misses the retained maximized member.
        let eligible_only: HashSet<String> = ["tok-sib".to_owned()].into_iter().collect();
        let eligible = m.eligible_focus_set(&members, &member_tokens, &eligible_only);
        assert_eq!(eligible, [sibling.clone()].into_iter().collect());
        // Retained-inclusive focus set (production union) keeps both; the
        // last-focus maximized mover wins the follow target.
        let focus_fresh: HashSet<String> = ["tok-sib".to_owned(), "tok-max".to_owned()]
            .into_iter()
            .collect();
        let focus_eligible = m.eligible_focus_set(&members, &member_tokens, &focus_fresh);
        assert_eq!(
            focus_eligible,
            [maxed.clone(), sibling.clone()].into_iter().collect()
        );
        assert_eq!(m.focus_target("mon-1", &ws2, &focus_eligible), Some(maxed));
    }

    #[test]
    fn sticky_only_intermediate_workspace_prunes_but_keeps_membership() {
        // Sticky members never occupy: a sticky-only intermediate workspace is
        // reported removed like an empty one, while the sticky membership
        // itself survives in the owner tables (retained float, rehomed by the
        // next sticky-off, dropped only by the member-drop path).
        use std::collections::BTreeSet;
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let first = m.active_id("mon-1").expect("active");
        let second = m.resolve_send("mon-1", 2).expect("second");
        // Occupy the trailing slot so a third workspace appends, leaving the
        // sticky-only second workspace intermediate (prunable, not floored).
        let normal_win = key(42);
        assert!(m.assign(normal_win.clone(), "mon-1", &second, false));
        let (third, _) = m.select_trailing("mon-1").expect("trailing");
        assert!(m.assign(normal_win.clone(), "mon-1", &third, false));
        let sticky_win = key(41);
        assert!(m.assign(sticky_win.clone(), "mon-1", &second, false));
        assert!(m.activate("mon-1", &first));
        let sticky: BTreeSet<WindowKey> = [sticky_win.clone()].into_iter().collect();
        // Without exclusion the intermediate workspace looks occupied and
        // survives; with sticky exclusion it is reported removed.
        let (kept, _) = m.plan_cleanup("mon-1", std::slice::from_ref(&first), &[]);
        assert!(
            !kept.contains(&second),
            "occupied intermediate survives, got {kept:?}"
        );
        let (removed, append) =
            m.plan_cleanup_excluding("mon-1", std::slice::from_ref(&first), &[], &sticky);
        assert_eq!(removed, vec![second.clone()], "sticky-only prunes");
        m.apply_cleanup("mon-1", &removed, append);
        assert_eq!(m.active_id("mon-1").as_deref(), Some(first.as_str()));
        // Membership is untouched by the plan: the owner retains the sticky
        // float for rehome on the next sticky-off.
        assert_eq!(
            m.member_loc(&sticky_win).map(|l| l.workspace.clone()),
            Some(second.clone())
        );
    }

    #[test]
    fn plan_cleanup_excluding_matches_plain_without_sticky() {
        // No sticky members: excluding degenerates to the plain plan.
        use std::collections::BTreeSet;
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let active = m.active_id("mon-1").expect("active");
        let plain = m.plan_cleanup("mon-1", std::slice::from_ref(&active), &[]);
        let excluded = m.plan_cleanup_excluding(
            "mon-1",
            std::slice::from_ref(&active),
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(plain, excluded);
    }

    #[test]
    fn workspace_mode_defaults_tiled_and_toggles_per_workspace() {
        let mut m = ManagedWorkspaces::new();
        assert!(m.default_tiled());
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        let ws2 = m.resolve_send("mon-1", 2).expect("ws2");
        assert!(m.is_tiled("mon-1", &ws1));
        assert!(m.is_tiled("mon-1", &ws2));
        assert!(m.set_tiled("mon-1", &ws1, false));
        assert!(!m.is_tiled("mon-1", &ws1));
        assert!(m.is_tiled("mon-1", &ws2));
        assert!(!m.set_tiled("mon-1", "ws-missing", false));
        assert!(!m.set_tiled("mon-missing", &ws1, false));
    }

    #[test]
    fn live_default_applies_only_to_future_workspaces() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        assert!(m.set_default_tiled(false));
        assert!(!m.set_default_tiled(false));
        // Existing workspaces keep their session state.
        assert!(m.is_tiled("mon-1", &ws1));
        // New workspaces seed from the live default.
        m.ensure_output("mon-2");
        let other = m.resolve_send("mon-2", 1).expect("other");
        assert!(!m.is_tiled("mon-2", &other));
        // Unknown workspaces read as the current default.
        assert!(!m.is_tiled("mon-2", "ws-missing"));
    }

    #[test]
    fn current_tiled_is_none_without_scope() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        assert_eq!(m.current_tiled("mon-1"), Some(true));
        assert_eq!(m.current_tiled("mon-missing"), None);
        let active = m.active_id("mon-1").expect("active");
        assert!(m.set_tiled("mon-1", &active, false));
        assert_eq!(m.current_tiled("mon-1"), Some(false));
    }

    #[test]
    fn cleanup_prunes_mode_state_with_workspaces() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let first = m.active_id("mon-1").expect("active");
        let second = m.resolve_send("mon-1", 2).expect("second");
        assert!(m.set_tiled("mon-1", &second, false));
        // An empty intermediate workspace prunes while survivors keep their
        // own session state; the pruned floating state leaves with it.
        assert!(m.assign(key(5), "mon-1", &first, false));
        assert!(m.assign(key(6), "mon-1", &second, false));
        let (third, created) = m.select_trailing("mon-1").expect("trailing");
        assert!(created);
        assert!(m.assign(key(6), "mon-1", &third, false));
        assert!(m.activate("mon-1", &first));
        let sticky: BTreeSet<WindowKey> = BTreeSet::new();
        let (removed, append) =
            m.plan_cleanup_excluding("mon-1", std::slice::from_ref(&first), &[], &sticky);
        assert!(removed.contains(&second));
        m.apply_cleanup("mon-1", &removed, append);
        assert!(!m.tiled.contains_key(&second));
        assert!(m.is_tiled("mon-1", &first));
    }

    #[test]
    fn floating_workspaces_keep_project_membership_for_new_windows() {
        // New windows admitted to a floating workspace join project
        // membership (hide/reveal scope) with frames untouched: mode never
        // gates assignment, only geometry/effects.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ws1 = m.resolve_send("mon-1", 1).expect("ws1");
        assert!(m.set_tiled("mon-1", &ws1, false));
        let w = key(31);
        assert!(m.assign(w.clone(), "mon-1", &ws1, false));
        assert_eq!(m.member_loc(&w).expect("loc").workspace, ws1);
        assert!(!m.is_tiled("mon-1", &ws1));
        assert!(!m.is_hidden(&w));
    }

    #[test]
    fn pending_releases_keep_latest_intent_per_domain() {
        // A failed float release queued as non-retile is superseded by the
        // later retile intent for the same domain: exactly one entry rides,
        // so a retry can never fire the stale float at a retiled layout.
        let mut pending = PendingReleases::new();
        assert!(pending.is_empty());
        pending.queue("mon-1", "ws-1", false);
        pending.queue("mon-1", "ws-1", true);
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending.snapshot(),
            vec![("mon-1".to_owned(), "ws-1".to_owned(), true)]
        );
        // A confirmed toggle clears its domain only.
        pending.queue("mon-1", "ws-2", false);
        assert!(pending.confirm("mon-1", "ws-1"));
        assert!(!pending.confirm("mon-1", "ws-1"));
        assert_eq!(
            pending.snapshot(),
            vec![("mon-1".to_owned(), "ws-2".to_owned(), false)]
        );
    }

    #[test]
    fn pending_releases_go_stale_once_tiled() {
        // Both intents imply a floating workspace: a workspace reading tiled
        // again must never release. Unknown workspaces still release (empty
        // rows clean up the orphaned Engine session).
        assert!(PendingReleases::is_stale(Some(true)));
        assert!(!PendingReleases::is_stale(Some(false)));
        assert!(!PendingReleases::is_stale(None));
    }

    #[test]
    fn topology_fingerprint_moves_only_with_topology() {
        let areas = vec![("mon-1".to_owned(), (0, 0, 800, 600), (0, 0, 800, 600))];
        let workspaces = vec![("mon-1".to_owned(), vec!["ws-1".to_owned()])];
        let members = vec![key(1)];
        let base = topology_fingerprint(&areas, &workspaces, &members, 1);
        assert_eq!(topology_fingerprint(&areas, &workspaces, &members, 1), base);
        // Member swap at the same count still moves: a same-count change is
        // a topology edge, never a quiet tick.
        assert_ne!(
            topology_fingerprint(&areas, &workspaces, &[key(2)], 1),
            base
        );
        assert_ne!(topology_fingerprint(&areas, &workspaces, &members, 2), base);
        let moved_areas = vec![("mon-1".to_owned(), (0, 0, 1024, 768), (0, 0, 1024, 768))];
        assert_ne!(
            topology_fingerprint(&moved_areas, &workspaces, &members, 1),
            base
        );
    }

    // Item 1 history/ring regression: observed changes record, failed
    // attempts and same-view activation never do, and the ring wraps the
    // full scoped order including trailing empty and ordinals beyond 9.

    fn history_ids(m: &ManagedWorkspaces, output: &str) -> Vec<String> {
        m.workspace_ids(output)
    }

    fn occupy_all_then_trailing(m: &mut ManagedWorkspaces, output: &str, count: usize) {
        for i in 0..count {
            let ids = history_ids(m, output);
            let mut placed = false;
            for ws in &ids {
                let occupied = m.workspace_members(output, ws).is_empty();
                if occupied {
                    assert!(m.assign(key(700 + i as u64), output, ws, false));
                    placed = true;
                    break;
                }
            }
            assert!(placed, "workspace available for {i}");
            let _ = m.select_trailing(output);
        }
    }

    #[test]
    fn history_records_only_observed_changes() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ids = history_ids(&m, "mon-1");
        let (ws1, ws2) = (ids[0].clone(), ids[1].clone());
        // Fresh output primes the baseline with no previous: toggle is a
        // no-op until the next recorded change, and setters never record.
        assert_eq!(m.history_baseline("mon-1").as_deref(), Some(ws1.as_str()));
        assert_eq!(m.history_previous("mon-1"), None);
        assert_eq!(m.resolve_previous("mon-1"), None);
        assert!(m.activate("mon-1", &ws1));
        assert!(!m.observe_workspace_change("mon-1", &ws1));
        assert_eq!(m.history_previous("mon-1"), None);
        // Unknown output/current never records.
        assert!(!m.observe_workspace_change("mon-gone", &ws1));
        assert!(!m.observe_workspace_change("mon-1", "ws-gone"));
        // First observed change records previous=ws1.
        assert!(m.activate("mon-1", &ws2));
        assert!(m.observe_workspace_change("mon-1", &ws2));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws1.as_str()));
        assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws1.as_str()));
    }

    #[test]
    fn history_repeated_toggle_walks_two_views() {
        // WS1->WS2->WS3->WS2->WS3: toggle alternates the last two observed
        // views (two-view toggle, not MRU traversal).
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ids = history_ids(&m, "mon-1");
        assert!(m.assign(key(701), "mon-1", &ids[0], false));
        assert!(m.assign(key(702), "mon-1", &ids[1], false));
        let (third, created) = m.select_trailing("mon-1").expect("third");
        assert!(created);
        // Trailing creation is itself an observed view change in production
        // (`workspace_do_select` records the verified transition).
        assert!(m.observe_workspace_change("mon-1", &third));
        assert!(m.activate("mon-1", &ids[0]));
        assert!(m.observe_workspace_change("mon-1", &ids[0]));
        let ids = history_ids(&m, "mon-1");
        let (ws1, ws2, ws3) = (ids[0].clone(), ids[1].clone(), third.clone());
        for target in [&ws2, &ws3, &ws2, &ws3] {
            assert!(m.activate("mon-1", target));
            assert!(m.observe_workspace_change("mon-1", target));
        }
        assert_eq!(m.history_baseline("mon-1").as_deref(), Some(ws3.as_str()));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws2.as_str()));
        // Toggle target resolves to ws2 without activating.
        let before = m.active_id("mon-1").expect("active");
        assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws2.as_str()));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(before.as_str()));
        assert!(m.activate("mon-1", &ws2));
        assert!(m.observe_workspace_change("mon-1", &ws2));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws3.as_str()));
        let _ = ws1;
    }

    #[test]
    fn history_same_view_and_output_focus_never_record() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let ids = history_ids(&m, "mon-1");
        assert!(m.activate("mon-1", &ids[1]));
        assert!(m.observe_workspace_change("mon-1", &ids[1]));
        // Re-observing the same view is a no-op: previous stays.
        assert!(!m.observe_workspace_change("mon-1", &ids[1]));
        assert_eq!(
            m.history_previous("mon-1").as_deref(),
            Some(ids[0].as_str())
        );
        // Merely focusing another output (no view change there) records
        // nothing on either output.
        assert!(!m.observe_workspace_change("mon-2", &history_ids(&m, "mon-2")[0]));
        assert_eq!(m.history_previous("mon-2"), None);
    }

    #[test]
    fn history_surviving_empty_stays_valid_but_removal_clears() {
        // Genuine lifecycle: visit ws2 (previous), empty it without removing
        // (stable id survives and still toggles), then prune it as an
        // intermediate empty through the real trailing plan. Removal clears
        // the previous entry: toggle is a no-op with no recreation.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ids = history_ids(&m, "mon-1");
        let (ws1, ws2) = (ids[0].clone(), ids[1].clone());
        // Occupy both so trailing appends a third workspace.
        let first_win = key(11);
        let second_win = key(12);
        assert!(m.assign(first_win.clone(), "mon-1", &ws1, false));
        assert!(m.assign(second_win.clone(), "mon-1", &ws2, false));
        let (ws3, created) = m.select_trailing("mon-1").expect("third");
        assert!(created);
        // Visit ws2, then return to ws1: previous is the visited ws2.
        assert!(m.activate("mon-1", &ws2));
        assert!(m.observe_workspace_change("mon-1", &ws2));
        assert!(m.activate("mon-1", &ws1));
        assert!(m.observe_workspace_change("mon-1", &ws1));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws2.as_str()));
        // Empty the previous workspace without removing it: the stable id
        // survives emptiness and still resolves.
        assert!(m.remove_window(&second_win));
        assert!(m.workspace_members("mon-1", &ws2).is_empty());
        assert_eq!(m.resolve_previous("mon-1").as_deref(), Some(ws2.as_str()));
        // Order is now [ws1 occupied, ws2 empty intermediate, ws3 trailing
        // empty] with ws1 active: the real plan prunes exactly ws2.
        let (removed, append) = m.plan_cleanup("mon-1", std::slice::from_ref(&ws1), &[]);
        assert_eq!(removed, vec![ws2.clone()]);
        assert!(!append);
        m.apply_cleanup("mon-1", &removed, append);
        // Inventory is stable with no recreation: survivors keep their ids,
        // the active view is untouched, and the removed previous clears.
        assert_eq!(m.workspace_ids("mon-1"), vec![ws1.clone(), ws3.clone()]);
        assert_eq!(m.active_id("mon-1").as_deref(), Some(ws1.as_str()));
        assert_eq!(m.history_previous("mon-1"), None);
        assert_eq!(m.resolve_previous("mon-1"), None);
        let _ = first_win;
    }

    #[test]
    fn history_disconnect_discards_and_reconnect_primes_fresh() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let ids = history_ids(&m, "mon-1");
        assert!(m.activate("mon-1", &ids[1]));
        assert!(m.observe_workspace_change("mon-1", &ids[1]));
        assert!(m.history_previous("mon-1").is_some());
        // Disconnect drops BOTH previous and baseline for the origin.
        assert!(m.displace_output_to("mon-1", "mon-2"));
        assert_eq!(m.history_previous("mon-1"), None);
        assert_eq!(m.history_baseline("mon-1"), None);
        // Reconnect primes a fresh baseline with no previous: toggle is a
        // no-op until the next recorded change, and selection never
        // consulted history (active is the first returning workspace).
        assert!(m.reconnect_output("mon-1"));
        assert_eq!(m.history_previous("mon-1"), None);
        let active = m.active_id("mon-1").expect("active");
        assert_eq!(
            m.history_baseline("mon-1").as_deref(),
            Some(active.as_str())
        );
        assert_eq!(m.resolve_previous("mon-1"), None);
        // The next observed displacement/return change records from the
        // fresh baseline.
        let ids = history_ids(&m, "mon-1");
        let other = ids.iter().find(|id| *id != &active).expect("other").clone();
        assert!(m.activate("mon-1", &other));
        assert!(m.observe_workspace_change("mon-1", &other));
        assert_eq!(
            m.history_previous("mon-1").as_deref(),
            Some(active.as_str())
        );
    }

    #[test]
    fn ring_wraps_trailing_and_beyond_nine_without_creation() {
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        occupy_all_then_trailing(&mut m, "mon-1", 10);
        assert!(m.workspace_count("mon-1") > 9);
        let ring = m.scoped_ring_ids("mon-1");
        assert_eq!(ring, history_ids(&m, "mon-1"));
        let first = ring.first().expect("first").clone();
        let last = ring.last().expect("last").clone();
        // Last -> next wraps to first; first -> previous wraps to last.
        assert!(m.activate("mon-1", &last));
        assert_eq!(
            m.resolve_relative("mon-1", 1).as_deref(),
            Some(first.as_str())
        );
        assert!(m.activate("mon-1", &first));
        assert_eq!(
            m.resolve_relative("mon-1", -1).as_deref(),
            Some(last.as_str())
        );
        // Resolution creates nothing and refuses bad deltas.
        let count = m.workspace_count("mon-1");
        assert_eq!(m.resolve_relative("mon-1", 0), None);
        assert_eq!(m.resolve_relative("mon-1", 2), None);
        assert_eq!(m.workspace_count("mon-1"), count);
        // Unknown output refuses.
        assert_eq!(m.resolve_relative("mon-gone", 1), None);
    }

    #[test]
    fn history_per_output_isolation_and_shared_scope() {
        // Local/global-unique model: per-output histories never leak.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let a = history_ids(&m, "mon-1");
        let b = history_ids(&m, "mon-2");
        assert!(m.activate("mon-1", &a[1]));
        assert!(m.observe_workspace_change("mon-1", &a[1]));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(a[0].as_str()));
        assert_eq!(m.history_previous("mon-2"), None);
        // Output focus alone on mon-2 records nothing.
        assert!(!m.observe_workspace_change("mon-2", &b[0]));
        // Shared model (future non-local runtime): one history/ring over the
        // full order, same record/no-op/invalidate rules, pure only.
        let live = vec!["s1".to_owned(), "s2".to_owned(), "s3".to_owned()];
        assert!(!m.observe_shared_change(&live, "s1"));
        assert!(m.resolve_shared_previous(&live).is_none());
        assert!(m.observe_shared_change(&live, "s2"));
        assert_eq!(m.resolve_shared_previous(&live).as_deref(), Some("s1"));
        assert_eq!(
            ManagedWorkspaces::resolve_shared_relative(&live, "s3", 1).as_deref(),
            Some("s1")
        );
        assert_eq!(
            ManagedWorkspaces::resolve_shared_relative(&live, "s1", -1).as_deref(),
            Some("s3")
        );
        // Removed shared ids clear through the genuine path, never
        // reinterpret: record s2 as previous against the full ring, then
        // resolve against a ring without it.
        let short = vec!["s1".to_owned(), "s3".to_owned()];
        assert_eq!(m.resolve_shared_previous(&short).as_deref(), Some("s1"));
        assert!(m.observe_shared_change(&live, "s3"));
        assert_eq!(m.resolve_shared_previous(&live).as_deref(), Some("s2"));
        assert_eq!(m.resolve_shared_previous(&short), None);
        assert_eq!(m.shared_previous, None);
    }

    #[test]
    fn history_moved_previous_clears_like_removal() {
        // A previous entry that moves to another output's scope clears on
        // the recording output (return-on-reconnect case): toggle is a
        // no-op until the next recorded change.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let ids = history_ids(&m, "mon-1");
        assert!(m.activate("mon-1", &ids[1]));
        assert!(m.observe_workspace_change("mon-1", &ids[1]));
        assert!(m.displace_output_to("mon-1", "mon-2"));
        assert!(m.reconnect_output("mon-1"));
        assert_eq!(m.history_previous("mon-1"), None);
        // Unchanged inventory: displacement/return moved workspaces without
        // creating or duplicating ids on the survivor.
        let survivor = history_ids(&m, "mon-2");
        let mut seen = survivor.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), survivor.len());
    }

    fn migration_fixture() -> (ManagedWorkspaces, Vec<String>, Vec<String>) {
        // Two outputs, three workspaces on the source: ws1 occupied, ws2
        // active with two members and remembered focus, ws3 trailing empty.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let seed = history_ids(&m, "mon-1");
        let dst = history_ids(&m, "mon-2");
        assert!(m.assign(key(21), "mon-1", &seed[0], false));
        let mover_a = key(22);
        let mover_b = key(23);
        assert!(m.assign(mover_a.clone(), "mon-1", &seed[1], false));
        assert!(m.assign(mover_b.clone(), "mon-1", &seed[1], true));
        m.note_foreground(&mover_a);
        // Occupy the trailing empty so the next spare appends a third
        // workspace; the move then migrates the middle entry.
        let (third, created) = m.select_trailing("mon-1").expect("third");
        assert!(created);
        assert!(m.activate("mon-1", &seed[1]));
        let src = history_ids(&m, "mon-1");
        assert_eq!(src.len(), 3);
        assert_eq!(src[2], third);
        (m, src, dst)
    }

    #[test]
    fn migrate_preserves_state_and_inserts_after_target_current() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, dst) = migration_fixture();
        let (ws1, ws2) = (src[0].clone(), src[1].clone());
        // Distinct tiling state per workspace survives the move.
        assert!(m.set_tiled("mon-1", &ws2, false));
        assert!(m.set_tiled("mon-2", &dst[0], false));
        // Target current is its second workspace; the moved entry lands
        // immediately after it and shows.
        assert!(m.activate("mon-2", &dst[1]));
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
            .expect("migrate");
        assert_eq!(outcome.moved_id, ws2);
        assert_eq!(outcome.source_output, "mon-1");
        assert_eq!(outcome.target_output, "mon-2");
        // Destination order: [dst0, dst1, ws2-after-current]; moved shows.
        assert_eq!(
            m.workspace_ids("mon-2"),
            vec![dst[0].clone(), dst[1].clone(), ws2.clone()]
        );
        assert_eq!(m.active_id("mon-2").as_deref(), Some(ws2.as_str()));
        // The previous target workspace stays listed (hidden, not removed).
        assert!(m.workspace_ids("mon-2").contains(&dst[1]));
        // Stable identity: members, remembered focus and tiling state travel
        // with the id; nothing is re-admitted.
        assert_eq!(
            m.workspace_members("mon-2", &ws2),
            [key(22), key(23)].into_iter().collect()
        );
        let moved_entry = m
            .outputs
            .get("mon-2")
            .expect("target")
            .order
            .iter()
            .find(|e| e.id == ws2)
            .expect("moved entry");
        assert_eq!(moved_entry.last_focus, Some(key(22)));
        assert_eq!(m.member_loc(&key(22)).expect("loc").output, "mon-2");
        assert!(m.member_loc(&key(23)).expect("loc").hidden);
        assert!(!m.is_tiled("mon-2", &ws2));
        assert!(!m.is_tiled("mon-2", &dst[0]));
        // Source keeps its remaining workspaces with no recreation.
        assert_eq!(m.workspace_ids("mon-1"), vec![ws1.clone(), src[2].clone()]);
        let _ = ws1;
    }

    #[test]
    fn migrate_default_refill_selects_last_remaining() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        // No recorded history: the default refills the last remaining scoped
        // entry (the trailing empty here).
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(src[2].as_str()));
    }

    #[test]
    fn migrate_mru_prefers_eligible_previous_over_trailing() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        let (ws1, ws2) = (src[0].clone(), src[1].clone());
        // The fixture shows ws2 without an observation yet: observing it
        // records ws1 as the previous view (baseline was the initial ws1).
        assert!(m.observe_workspace_change("mon-1", &ws2));
        assert_eq!(m.history_previous("mon-1").as_deref(), Some(ws1.as_str()));
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(ws1.as_str()));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(ws1.as_str()));
    }

    #[test]
    fn migrate_mru_falls_back_without_eligible_previous() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        // Case 1: no history at all falls back to last remaining.
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
        // Case 2: a previous entry that left scope falls
        // back instead of recreating.
        let (mut m, src, _) = migration_fixture();
        m.previous.insert("mon-1".to_owned(), "ws-gone".to_owned());
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
        assert_eq!(m.workspace_ids("mon-1").len(), 2);
    }

    #[test]
    fn migrate_mru_accepts_surviving_empty_previous() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        // Visit the trailing empty, then return to ws2: the remembered empty
        // survives in scope and refills under MRU.
        let empty = src[2].clone();
        let ws2 = src[1].clone();
        assert!(m.activate("mon-1", &empty));
        assert!(m.observe_workspace_change("mon-1", &empty));
        assert!(m.activate("mon-1", &ws2));
        assert!(m.observe_workspace_change("mon-1", &ws2));
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(empty.as_str()));
        assert_eq!(m.active_id("mon-1").as_deref(), Some(empty.as_str()));
    }

    #[test]
    fn migrate_uses_output_local_history_only() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, dst) = migration_fixture();
        // mon-2 records its own previous; the mon-1 migration must not
        // consult it (per-output scope, never global).
        assert!(m.activate("mon-2", &dst[1]));
        assert!(m.observe_workspace_change("mon-2", &dst[1]));
        assert_eq!(
            m.history_previous("mon-2").as_deref(),
            Some(dst[0].as_str())
        );
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::MostRecentlyUsed)
            .expect("migrate");
        assert_eq!(outcome.source_refill.as_deref(), Some(src[2].as_str()));
        assert!(m.history_previous("mon-2").is_some());
    }

    #[test]
    fn migrate_empty_source_reports_no_refill_without_recreation() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        m.ensure_output("mon-2");
        let ids = history_ids(&m, "mon-1");
        // Trim the source to a single active workspace through the struct's
        // own order (test-only): migrating it leaves no remaining scope, so
        // both modes report no refill and the source keeps an empty order
        // for ordinary lifecycle to converge (nothing recreated here).
        {
            let state = m.outputs.get_mut("mon-1").expect("source");
            state.order.truncate(1);
            state.active = 0;
        }
        for refill in [Refill::LastRemaining, Refill::MostRecentlyUsed] {
            let mut m = m.clone();
            let outcome = m
                .migrate_workspace("mon-1", "mon-2", refill)
                .expect("migrate");
            assert_eq!(outcome.moved_id, ids[0]);
            assert_eq!(outcome.source_refill, None);
            assert!(m.workspace_ids("mon-1").is_empty());
            assert!(m.workspace_ids("mon-2").contains(&ids[0]));
        }
    }

    #[test]
    fn migrate_invalidates_moved_history_and_keeps_sibling_return() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        let (ws1, ws2) = (src[0].clone(), src[1].clone());
        // Previous points at the workspace about to move: after migration it
        // clears like removal (out of source scope).
        m.previous.insert("mon-1".to_owned(), ws2.clone());
        // Sibling hotplug-return association on another origin survives; the
        // moved id loses auto return.
        m.displaced.insert(
            "mon-gone".to_owned(),
            DisplacedRecord {
                workspace_ids: vec![ws1.clone(), "ws-other".to_owned()],
                dest_key: "mon-1".to_owned(),
            },
        );
        m.displaced.insert(
            "mon-old".to_owned(),
            DisplacedRecord {
                workspace_ids: vec![ws2.clone()],
                dest_key: "mon-2".to_owned(),
            },
        );
        let outcome = m
            .migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
            .expect("migrate");
        assert_eq!(outcome.moved_id, ws2);
        assert_eq!(m.history_previous("mon-1"), None);
        let gone = m.displaced.get("mon-gone").expect("sibling record");
        assert_eq!(gone.workspace_ids, vec![ws1.clone(), "ws-other".to_owned()]);
        assert!(!m.displaced.contains_key("mon-old"));
        let _ = ws1;
    }

    #[test]
    fn migrate_refuses_same_unknown_or_empty_source() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, _, _) = migration_fixture();
        let before = m.displaced_snapshot();
        assert!(
            m.migrate_workspace("mon-1", "mon-1", Refill::LastRemaining)
                .is_none()
        );
        assert!(
            m.migrate_workspace("mon-1", "mon-gone", Refill::LastRemaining)
                .is_none()
        );
        assert!(
            m.migrate_workspace("mon-gone", "mon-2", Refill::LastRemaining)
                .is_none()
        );
        // Refusals change nothing, including return associations.
        assert_eq!(m.displaced_snapshot(), before);
        assert_eq!(m.workspace_ids("mon-1").len(), 3);
    }

    #[test]
    fn migrate_excluding_keeps_sticky_on_source_refill() {
        use tiler_core::output_selection::MigrationSourceRefill as Refill;
        let (mut m, src, _) = migration_fixture();
        let ws2 = src[1].clone();
        // The hidden member of the moved workspace is sticky: the filtered
        // primitive strips it from the moved entry and re-homes it to the
        // selected source refill with hidden state preserved.
        let sticky = key(23);
        let sticky_set: BTreeSet<WindowKey> = [sticky.clone()].into_iter().collect();
        let outcome = m
            .migrate_workspace_excluding("mon-1", "mon-2", Refill::LastRemaining, &sticky_set)
            .expect("migrate");
        assert_eq!(outcome.moved_id, ws2);
        let target_members = m.workspace_members("mon-2", &ws2);
        assert!(
            !target_members.contains(&sticky),
            "sticky never rides the moved entry"
        );
        assert!(
            target_members.contains(&key(22)),
            "migrating member travels"
        );
        let refill = outcome.source_refill.expect("refill");
        let loc = m.member_loc(&sticky).expect("sticky homed");
        assert_eq!(loc.output, "mon-1");
        assert_eq!(loc.workspace, refill);
        assert!(m.is_hidden(&sticky), "sticky hidden state preserved");
        assert!(m.workspace_members("mon-1", &refill).contains(&sticky));
        // The unfiltered wrapper keeps prior behavior (sticky travels with
        // the entry) so existing callers are unaffected.
        let (mut m, src, _) = migration_fixture();
        let ws2 = src[1].clone();
        m.migrate_workspace("mon-1", "mon-2", Refill::LastRemaining)
            .expect("migrate");
        assert!(m.workspace_members("mon-2", &ws2).contains(&key(23)));
    }
}
